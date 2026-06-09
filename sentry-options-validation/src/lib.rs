//! Schema validation library for sentry-options.
//!
//! Schemas are loaded once into a [`SchemaRegistry`] and shared via `Arc`.
//! Values are validated against schemas as complete objects.
//!
//! # Refresh-on-read scheme
//!
//! Values are held in a [`ValuesStore`] that wraps an [`ArcSwap`] for
//! lock-free reads. There is no background thread — every `load()` decides
//! whether the cached snapshot is stale and, if so, the calling thread
//! refreshes it.
//!
//! Each `load()` does:
//!
//! 1. Read `now` and `last_updated` (an `AtomicU64` of nanoseconds since a
//!    monotonic baseline) with `Acquire`.
//! 2. Compute a per-call jitter in `[0, 1s)` from the address of a stack
//!    local — different threads have different stack bases, so the value
//!    differs across threads. No `thread_local` is involved.
//! 3. If `now - last_updated < threshold + jitter`, return the current
//!    snapshot. Otherwise:
//!    a. Read all values files from disk and build the new map.
//!    b. On success, publish the new map into the `ArcSwap`
//!    (last-writer-wins under contention).
//!    c. `compare_exchange` `last_updated` from the previously-observed
//!    value to `now` with `AcqRel`. The Release on the timestamp
//!    publishes the prior `ArcSwap::store`: any reader that
//!    Acquire-loads the bumped timestamp and short-circuits the
//!    refresh is guaranteed to subsequently load the new snapshot.
//!    d. On a parse/validation failure, leave the old map in place — the
//!    bumped timestamp makes other threads back off until the next
//!    window.
//!
//! Multiple threads racing through the stale window will redundantly read
//! files and publish; the last `ArcSwap::store` wins. The jitter spreads
//! the threshold boundary so that the herd doesn't cross it together. On
//! error the timestamp is still bumped so a broken values directory does
//! not turn into an I/O storm.

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use serde_json::Value;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

/// Embedded meta-schema for validating sentry-options schema files
const NAMESPACE_SCHEMA_JSON: &str = include_str!("namespace-schema.json");

/// Embedded Feature type definitions for injecting into namespace schemas that contain feature flags
const FEATURE_SCHEMA_DEFS_JSON: &str = include_str!("feature-schema-defs.json");

const SCHEMA_FILE_NAME: &str = "schema.json";
const VALUES_FILE_NAME: &str = "values.json";

/// Default minimum age of a cached values snapshot before a read triggers a
/// refresh. A per-thread jitter of up to 1 s is added on top to spread the
/// reload across threads.
const REFRESH_THRESHOLD: Duration = Duration::from_secs(5);

/// Production path where options are deployed via config map
pub const PRODUCTION_OPTIONS_DIR: &str = "/etc/sentry-options";

/// Local fallback path for development
pub const LOCAL_OPTIONS_DIR: &str = "sentry-options";

/// Environment variable to override options directory
pub const OPTIONS_DIR_ENV: &str = "SENTRY_OPTIONS_DIR";

/// Environment variable to suppress missing directory errors
pub const OPTIONS_SUPPRESS_MISSING_DIR_ENV: &str = "SENTRY_OPTIONS_SUPPRESS_MISSING_DIR";

/// Check if missing directory errors should be suppressed
fn should_suppress_missing_dir_errors() -> bool {
    std::env::var(OPTIONS_SUPPRESS_MISSING_DIR_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Resolve options directory using fallback chain:
/// 1. `SENTRY_OPTIONS_DIR` env var (if set)
/// 2. `/etc/sentry-options` (if exists)
/// 3. `sentry-options/` (local fallback)
pub fn resolve_options_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(OPTIONS_DIR_ENV) {
        return PathBuf::from(dir);
    }

    let prod_path = PathBuf::from(PRODUCTION_OPTIONS_DIR);
    if prod_path.exists() {
        return prod_path;
    }

    PathBuf::from(LOCAL_OPTIONS_DIR)
}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// A map of option values keyed by their namespace
pub type ValuesByNamespace = HashMap<String, HashMap<String, Value>>;

/// Errors that can occur during schema and value validation
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Schema error in {file}: {message}")]
    SchemaError { file: PathBuf, message: String },

    #[error("Value error for {namespace}: {errors}")]
    ValueError { namespace: String, errors: String },

    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Unknown option '{key}' in namespace '{namespace}'")]
    UnknownOption { namespace: String, key: String },

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    JSONParse(#[from] serde_json::Error),

    #[error("{} validation error(s)", .0.len())]
    ValidationErrors(Vec<ValidationError>),

    #[error("Invalid {label} '{name}': {reason}")]
    InvalidName {
        label: String,
        name: String,
        reason: String,
    },
}

/// Validate a name component is valid for K8s (lowercase alphanumeric, '-', '.')
pub fn validate_k8s_name_component(name: &str, label: &str) -> ValidationResult<()> {
    if let Some(c) = name
        .chars()
        .find(|&c| !matches!(c, 'a'..='z' | '0'..='9' | '-' | '.'))
    {
        return Err(ValidationError::InvalidName {
            label: label.to_string(),
            name: name.to_string(),
            reason: format!(
                "character '{}' not allowed. Use lowercase alphanumeric, '-', or '.'",
                c
            ),
        });
    }
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric())
        || !name.ends_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(ValidationError::InvalidName {
            label: label.to_string(),
            name: name.to_string(),
            reason: "must start and end with alphanumeric".to_string(),
        });
    }
    Ok(())
}

/// Metadata for a single option in a namespace schema
#[derive(Debug, Clone)]
pub struct OptionMetadata {
    pub option_type: String,
    pub property_schema: Value,
    pub default: Value,
}

/// Schema for a namespace, containing validator and option metadata
pub struct NamespaceSchema {
    pub namespace: String,
    pub options: HashMap<String, OptionMetadata>,
    /// All property keys from the schema, including feature flags that aren't in `options`.
    all_keys: HashSet<String>,
    validator: jsonschema::Validator,
}

impl NamespaceSchema {
    /// Validate an entire values object against this schema
    ///
    /// # Arguments
    /// * `values` - JSON object containing option key-value pairs
    ///
    /// # Errors
    /// Returns error if values don't match the schema
    pub fn validate_values(&self, values: &Value) -> ValidationResult<()> {
        let output = self.validator.evaluate(values);
        if output.flag().valid {
            Ok(())
        } else {
            let errors: Vec<String> = output
                .iter_errors()
                .map(|e| {
                    format!(
                        "\n\t{} {}",
                        e.instance_location.as_str().trim_start_matches("/"),
                        e.error
                    )
                })
                .collect();
            Err(ValidationError::ValueError {
                namespace: self.namespace.clone(),
                errors: errors.join(""),
            })
        }
    }

    /// Get the default value for an option key.
    /// Returns None if the key doesn't exist in the schema.
    pub fn get_default(&self, key: &str) -> Option<&Value> {
        self.options.get(key).map(|meta| &meta.default)
    }

    /// Validate a single key-value pair against the schema.
    ///
    /// # Errors
    /// Returns error if the key doesn't exist or the value doesn't match the expected type.
    pub fn validate_option(&self, key: &str, value: &Value) -> ValidationResult<()> {
        if !self.options.contains_key(key) {
            return Err(ValidationError::UnknownOption {
                namespace: self.namespace.clone(),
                key: key.to_string(),
            });
        }
        let test_obj = json!({ key: value });
        self.validate_values(&test_obj)
    }
}

/// Registry for loading and storing schemas
pub struct SchemaRegistry {
    schemas: HashMap<String, Arc<NamespaceSchema>>,
}

impl SchemaRegistry {
    /// Create a new empty schema registry
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Load schemas from a directory and create a registry
    ///
    /// Expects directory structure: `schemas/{namespace}/schema.json`
    ///
    /// # Arguments
    /// * `schemas_dir` - Path to directory containing namespace subdirectories
    ///
    /// # Errors
    /// Returns error if directory doesn't exist or any schema is invalid
    pub fn from_directory(schemas_dir: &Path) -> ValidationResult<Self> {
        let namespace_validator = Self::compile_namespace_validator()?;
        let mut schemas_map = HashMap::new();

        // TODO: Parallelize the loading of schemas for the performance gainz
        for entry in fs::read_dir(schemas_dir)? {
            let entry = entry?;

            if !entry.file_type()?.is_dir() {
                continue;
            }

            let namespace =
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| ValidationError::SchemaError {
                        file: entry.path(),
                        message: "Directory name contains invalid UTF-8".to_string(),
                    })?;

            validate_k8s_name_component(&namespace, "namespace name")?;

            let schema_file = entry.path().join(SCHEMA_FILE_NAME);
            let file = fs::File::open(&schema_file)?;
            let schema_data: Value = serde_json::from_reader(file)?;

            Self::validate_with_namespace_schema(&schema_data, &schema_file, &namespace_validator)?;
            let schema = Self::parse_schema(schema_data, &namespace, &schema_file)?;
            schemas_map.insert(namespace, schema);
        }

        Ok(Self {
            schemas: schemas_map,
        })
    }

    /// Build a registry from in-memory schema JSON strings.
    ///
    /// Each entry is a `(namespace, json)` pair. Applies the same validation
    /// pipeline as `from_directory` (meta-schema check, constraint injection,
    /// validator compilation) without reading from the filesystem.
    pub fn from_schemas(schemas: &[(&str, &str)]) -> ValidationResult<Self> {
        let namespace_validator = Self::compile_namespace_validator()?;
        let schema_file = Path::new("<embedded>");
        let mut schemas_map = HashMap::new();

        for (namespace, json) in schemas {
            validate_k8s_name_component(namespace, "namespace name")?;

            let schema_data: Value =
                serde_json::from_str(json).map_err(|e| ValidationError::SchemaError {
                    file: schema_file.to_path_buf(),
                    message: format!("Invalid JSON for namespace '{}': {}", namespace, e),
                })?;

            Self::validate_with_namespace_schema(&schema_data, schema_file, &namespace_validator)?;
            let schema = Self::parse_schema(schema_data, namespace, schema_file)?;
            if schemas_map.insert(namespace.to_string(), schema).is_some() {
                return Err(ValidationError::SchemaError {
                    file: schema_file.to_path_buf(),
                    message: format!("Duplicate namespace '{}'", namespace),
                });
            }
        }

        Ok(Self {
            schemas: schemas_map,
        })
    }

    /// Validate an entire values object for a namespace
    pub fn validate_values(&self, namespace: &str, values: &Value) -> ValidationResult<()> {
        let schema = self
            .schemas
            .get(namespace)
            .ok_or_else(|| ValidationError::UnknownNamespace(namespace.to_string()))?;

        schema.validate_values(values)
    }

    fn compile_namespace_validator() -> ValidationResult<jsonschema::Validator> {
        let namespace_schema_value: Value =
            serde_json::from_str(NAMESPACE_SCHEMA_JSON).map_err(|e| {
                ValidationError::InternalError(format!("Invalid namespace-schema JSON: {}", e))
            })?;
        jsonschema::validator_for(&namespace_schema_value).map_err(|e| {
            ValidationError::InternalError(format!("Failed to compile namespace-schema: {}", e))
        })
    }

    /// Validate a schema against the namespace-schema
    fn validate_with_namespace_schema(
        schema_data: &Value,
        path: &Path,
        namespace_validator: &jsonschema::Validator,
    ) -> ValidationResult<()> {
        let output = namespace_validator.evaluate(schema_data);

        if output.flag().valid {
            Ok(())
        } else {
            let errors: Vec<String> = output
                .iter_errors()
                .map(|e| format!("Error: {}", e.error))
                .collect();

            Err(ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!("Schema validation failed:\n{}", errors.join("\n")),
            })
        }
    }

    /// Validate that a default value matches its declared type using jsonschema
    fn validate_default_type(
        property_name: &str,
        property_schema: &Value,
        default_value: &Value,
        path: &Path,
    ) -> ValidationResult<()> {
        // Validate the default value against the property schema
        jsonschema::validate(property_schema, default_value).map_err(|e| {
            ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!(
                    "Property '{}': default value does not match schema: {}",
                    property_name, e
                ),
            }
        })?;

        Ok(())
    }

    /// Injects `required` (all non-optional field names) into an object-typed schema.
    /// Also injects `additionalProperties: false` unless the schema already declares
    /// `additionalProperties` explicitly — that signals a dynamic map where keys are
    /// not known up front (e.g. `"additionalProperties": {"type": "string"}`).
    /// e.g.
    /// {
    ///     "type": "object",
    ///     "properties": {
    ///       "host": { "type": "string" },
    ///       "port": { "type": "integer" }
    ///     },
    ///     "required": ["host", "port"],                       <-- INJECTED
    ///     "additionalProperties": false,                      <-- INJECTED
    ///     "default": { "host": "localhost", "port": 8080 },
    ///     "description": "..."
    /// }
    fn inject_object_constraints(schema: &mut Value) {
        if let Some(obj) = schema.as_object_mut() {
            if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
                let required: Vec<Value> = props
                    .iter()
                    .filter(|(_, v)| !v.get("optional").and_then(|o| o.as_bool()).unwrap_or(false))
                    .map(|(k, _)| Value::String(k.clone()))
                    .collect();
                obj.insert("required".to_string(), Value::Array(required));
            }
            if !obj.contains_key("additionalProperties") {
                obj.insert("additionalProperties".to_string(), json!(false));
            }
        }
    }

    /// Parse a schema JSON into NamespaceSchema
    fn parse_schema(
        mut schema: Value,
        namespace: &str,
        path: &Path,
    ) -> ValidationResult<Arc<NamespaceSchema>> {
        // Inject additionalProperties: false to reject unknown options
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("additionalProperties".to_string(), json!(false));
        }

        // Inject object constraints (required + additionalProperties) for object-typed options
        // so that jsonschema validates the full shape of object values.
        if let Some(properties) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
            for prop_value in properties.values_mut() {
                let prop_type = prop_value
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                if prop_type == "object" {
                    Self::inject_object_constraints(prop_value);
                } else if prop_type == "array"
                    && let Some(items) = prop_value.get_mut("items")
                {
                    let items_type = items.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if items_type == "object" {
                        Self::inject_object_constraints(items);
                    }
                }
            }
        }

        // Extract option metadata and validate types.
        let mut options = HashMap::new();
        let mut all_keys = HashSet::new();
        let mut has_feature_keys = false;
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_value) in properties {
                all_keys.insert(prop_name.clone());
                // Detect feature flags so that we can augment the schema with defs.
                if prop_name.starts_with("feature.") {
                    has_feature_keys = true;
                }
                if let (Some(prop_type), Some(default_value)) = (
                    prop_value.get("type").and_then(|t| t.as_str()),
                    prop_value.get("default"),
                ) {
                    Self::validate_default_type(prop_name, prop_value, default_value, path)?;
                    options.insert(
                        prop_name.clone(),
                        OptionMetadata {
                            option_type: prop_type.to_string(),
                            property_schema: prop_value.clone(),
                            default: default_value.clone(),
                        },
                    );
                }
            }
        }

        // If an options schema includes a feature flag, splice in the definitions
        // so that values can be validated.
        if has_feature_keys {
            let feature_defs: Value =
                serde_json::from_str(FEATURE_SCHEMA_DEFS_JSON).map_err(|e| {
                    ValidationError::InternalError(format!(
                        "Invalid feature-schema-defs JSON: {}",
                        e
                    ))
                })?;

            if let Some(obj) = schema.as_object_mut() {
                obj.insert("definitions".to_string(), feature_defs);
            }
        }

        // Use the (potentially transformed) schema as the validator
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!("Failed to compile validator: {}", e),
            })?;

        Ok(Arc::new(NamespaceSchema {
            namespace: namespace.to_string(),
            options,
            all_keys,
            validator,
        }))
    }

    /// Get a namespace schema by name
    pub fn get(&self, namespace: &str) -> Option<&Arc<NamespaceSchema>> {
        self.schemas.get(namespace)
    }

    /// Get all loaded schemas (for schema evolution validation)
    pub fn schemas(&self) -> &HashMap<String, Arc<NamespaceSchema>> {
        &self.schemas
    }

    /// Load and validate JSON values from a directory.
    /// Allows extra unknown option values to accommodate deployment race conditions
    ///
    /// Expects structure: `{values_dir}/{namespace}/values.json`
    /// Values file must have format: `{"options": {"key": value, ...}, "generated_at": "..."}`
    /// Skips namespaces without a values.json file.
    /// Returns the values and a map of namespace -> `generated_at` timestamp.
    pub fn load_values_json(
        &self,
        values_dir: &Path,
    ) -> ValidationResult<(ValuesByNamespace, HashMap<String, String>)> {
        let mut all_values = HashMap::new();
        let mut generated_at_by_namespace: HashMap<String, String> = HashMap::new();

        for namespace in self.schemas.keys() {
            let values_file = values_dir.join(namespace).join(VALUES_FILE_NAME);

            if !values_file.exists() {
                continue;
            }

            let parsed: Value = serde_json::from_reader(fs::File::open(&values_file)?)?;

            // Extract generated_at if present
            if let Some(ts) = parsed.get("generated_at").and_then(|v| v.as_str()) {
                generated_at_by_namespace.insert(namespace.clone(), ts.to_string());
            }

            let values = parsed
                .get("options")
                .ok_or_else(|| ValidationError::ValueError {
                    namespace: namespace.clone(),
                    errors: "values.json must have an 'options' key".to_string(),
                })?;

            // Strip unknown keys before validation to handle deployment race
            // conditions where values are deployed before the schema update.
            let values = self.strip_unknown_keys(namespace, values);

            self.validate_values(namespace, &values)?;

            if let Value::Object(obj) = values {
                let ns_values: HashMap<String, Value> = obj.into_iter().collect();
                all_values.insert(namespace.clone(), ns_values);
            }
        }

        Ok((all_values, generated_at_by_namespace))
    }

    /// Remove keys from values that are not defined in the namespace schema.
    /// Logs a warning for each removed key. Returns the filtered values object.
    pub fn strip_unknown_keys(&self, namespace: &str, values: &Value) -> Value {
        let schema = match self.schemas.get(namespace) {
            Some(s) => s,
            None => return values.clone(),
        };

        let obj = match values.as_object() {
            Some(obj) => obj,
            None => return values.clone(),
        };

        let unknown_keys: Vec<&String> = obj
            .keys()
            .filter(|k| !schema.all_keys.contains(*k))
            .collect();

        if unknown_keys.is_empty() {
            return values.clone();
        }

        for key in &unknown_keys {
            eprintln!(
                "sentry-options: Ignoring unknown option '{}' in namespace '{}'. \
                 This is expected during deployments when values are updated before schemas.",
                key, namespace
            );
        }

        let filtered: serde_json::Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| schema.all_keys.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Value::Object(filtered)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazily reloads values from disk when reads detect they are stale.
///
/// Holds the current `ValuesByNamespace` snapshot in an `ArcSwap` for
/// lock-free reads. Every `load()` checks whether the snapshot is older than
/// `refresh_threshold + jitter`; if so, the calling thread reads the values
/// directory, then compare-and-swaps the timestamp. Whichever thread wins the
/// CAS publishes its snapshot into the `ArcSwap`; losers discard their work.
///
/// Replaces the polling watcher thread: idle processes do no work, and
/// concurrent readers coordinate through the timestamp CAS.
/// Callback invoked when a namespace's values are updated.
/// Arguments: `(namespace, delay_secs)` where `delay_secs` is the propagation
/// delay from ConfigMap generation to the app reading the new values.
pub type PropagationCallback = Box<dyn Fn(&str, f64) + Send + Sync>;

pub struct ValuesStore {
    registry: Arc<SchemaRegistry>,
    values_dir: PathBuf,
    values: ArcSwap<ValuesByNamespace>,
    baseline: Instant,
    last_refresh_offset_ns: AtomicU64,
    refresh_threshold: Duration,
    /// Last known `generated_at` per namespace, used to detect value changes.
    /// Uses ArcSwap for lock-free, fork-safe access.
    last_generated_at: ArcSwap<HashMap<String, String>>,
    /// Optional callback invoked when propagation delay is measured.
    on_propagation: Option<PropagationCallback>,
}

impl ValuesStore {
    /// Build a store and perform the initial values load synchronously.
    pub fn new(registry: Arc<SchemaRegistry>, values_dir: &Path) -> ValidationResult<Self> {
        Self::build(registry, values_dir, REFRESH_THRESHOLD, None)
    }

    /// Build a store with a callback that fires whenever new values are detected.
    /// The callback receives `(namespace, delay_secs)` on each value change.
    pub fn with_propagation_callback(
        registry: Arc<SchemaRegistry>,
        values_dir: &Path,
        callback: PropagationCallback,
    ) -> ValidationResult<Self> {
        Self::build(registry, values_dir, REFRESH_THRESHOLD, Some(callback))
    }

    /// Internal constructor. `Duration::ZERO` threshold makes every `load()`
    /// refresh, which is useful for tests.
    fn build(
        registry: Arc<SchemaRegistry>,
        values_dir: &Path,
        refresh_threshold: Duration,
        on_propagation: Option<PropagationCallback>,
    ) -> ValidationResult<Self> {
        if !should_suppress_missing_dir_errors() && fs::metadata(values_dir).is_err() {
            eprintln!("Values directory does not exist: {}", values_dir.display());
        }

        let baseline = Instant::now();
        let (initial, generated_at_by_namespace) = registry.load_values_json(values_dir)?;
        let last_refresh_offset_ns = AtomicU64::new(baseline.elapsed().as_nanos() as u64);

        Ok(Self {
            registry,
            values_dir: values_dir.to_path_buf(),
            values: ArcSwap::from_pointee(initial),
            baseline,
            last_refresh_offset_ns,
            refresh_threshold,
            last_generated_at: ArcSwap::from_pointee(generated_at_by_namespace),
            on_propagation,
        })
    }

    /// The registry the store was constructed with.
    pub fn registry(&self) -> &Arc<SchemaRegistry> {
        &self.registry
    }

    /// Returns a guard onto the current values snapshot, refreshing first if
    /// the cached snapshot is older than `refresh_threshold + jitter`.
    pub fn load(&self) -> arc_swap::Guard<Arc<ValuesByNamespace>> {
        self.maybe_refresh();
        self.values.load()
    }

    fn maybe_refresh(&self) {
        let now_ns = self.baseline.elapsed().as_nanos() as u64;
        let last_ns = self.last_refresh_offset_ns.load(Ordering::Acquire);
        let elapsed_ns = now_ns.saturating_sub(last_ns);
        let threshold_ns = self.refresh_threshold.as_nanos() as u64;
        // Skip jitter for a zero threshold so callers (chiefly tests) can
        // force every read to refresh.
        let jitter_ns = if self.refresh_threshold.is_zero() {
            0
        } else {
            stack_jitter_ns()
        };

        if elapsed_ns < threshold_ns.saturating_add(jitter_ns) {
            return;
        }

        self.refresh(last_ns, now_ns);
    }

    fn refresh(&self, observed_last_ns: u64, now_ns: u64) {
        let result = self.registry.load_values_json(&self.values_dir);

        // Publish the new snapshot before bumping the timestamp. The CAS below
        // uses AcqRel (Release on success); a reader that Acquire-loads the
        // bumped timestamp and decides the snapshot is fresh enough to skip
        // refreshing is then guaranteed to observe this `ArcSwap::store`.
        // Reversing the order opens a window where the timestamp says "fresh"
        // but the ArcSwap still holds the previous snapshot on weakly ordered
        // architectures.
        let new_generated_at = match result {
            Ok((new_values, generated_at)) => {
                self.values.store(Arc::new(new_values));
                Some(generated_at)
            }
            Err(e) => {
                eprintln!(
                    "Failed to reload values from {}: {}",
                    self.values_dir.display(),
                    e
                );
                None
            }
        };

        // Bump the timestamp regardless of success. On failure, the bumped
        // timestamp keeps subsequent reads from hammering the filesystem until
        // the next threshold window. Losing the CAS just means another thread
        // already bumped — our snapshot, if any, is still published.
        let _ = self.last_refresh_offset_ns.compare_exchange(
            observed_last_ns,
            now_ns,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        // Fire the propagation callback after the CAS so other threads see a
        // fresh timestamp and don't redundantly re-read from disk while a
        // potentially slow callback (e.g. Python/GIL) executes.
        if let Some(new_generated_at) = new_generated_at {
            let last = self.last_generated_at.load();
            if *last.as_ref() != new_generated_at {
                if let Some(callback) = &self.on_propagation {
                    let applied_at = Utc::now();
                    for (ns, ts) in &new_generated_at {
                        if last.get(ns).is_some_and(|old| old == ts) {
                            continue;
                        }
                        match propagation_delay_secs(&applied_at, ts) {
                            Some(delay) => {
                                if let Err(e) =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        callback(ns, delay)
                                    }))
                                {
                                    eprintln!("Propagation callback panicked for {ns}: {:?}", e);
                                }
                            }
                            None => eprintln!("Bad generated_at for {ns}: {ts}"),
                        }
                    }
                }
                self.last_generated_at.store(Arc::new(new_generated_at));
            }
        }
    }
}

/// Parse an RFC3339 timestamp and return the delay in seconds from then to `now`.
fn propagation_delay_secs(now: &DateTime<Utc>, generated_at: &str) -> Option<f64> {
    let generated = DateTime::parse_from_rfc3339(generated_at).ok()?;
    let delay = (*now - generated.with_timezone(&Utc)).num_milliseconds() as f64 / 1000.0;
    Some(delay.max(0.0))
}

/// Per-call jitter in `[0, 1s)` nanoseconds, derived from the address of a
/// stack local. Different threads have different stack bases, so the value
/// differs across threads; the same thread + call site stays roughly stable.
/// Cheap (~5 ns) and avoids `thread_local`.
fn stack_jitter_ns() -> u64 {
    let local = 0u8;
    let addr = &local as *const u8 as usize as u64;
    addr.wrapping_mul(0x9E37_79B9_7F4A_7C15) % 1_000_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn create_test_schema(temp_dir: &TempDir, namespace: &str, schema_json: &str) -> PathBuf {
        let schema_dir = temp_dir.path().join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, schema_json).unwrap();
        schema_file
    }

    fn create_test_schema_with_values(
        temp_dir: &TempDir,
        namespace: &str,
        schema_json: &str,
        values_json: &str,
    ) -> (PathBuf, PathBuf) {
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, schema_json).unwrap();

        let ns_values_dir = values_dir.join(namespace);
        fs::create_dir_all(&ns_values_dir).unwrap();
        let values_file = ns_values_dir.join("values.json");
        fs::write(&values_file, values_json).unwrap();

        (schemas_dir, values_dir)
    }

    #[test]
    fn test_validate_k8s_name_component_valid() {
        assert!(validate_k8s_name_component("relay", "namespace").is_ok());
        assert!(validate_k8s_name_component("my-service", "namespace").is_ok());
        assert!(validate_k8s_name_component("my.service", "namespace").is_ok());
        assert!(validate_k8s_name_component("a1-b2.c3", "namespace").is_ok());
    }

    #[test]
    fn test_validate_k8s_name_component_rejects_uppercase() {
        let result = validate_k8s_name_component("MyService", "namespace");
        assert!(matches!(result, Err(ValidationError::InvalidName { .. })));
        assert!(result.unwrap_err().to_string().contains("'M' not allowed"));
    }

    #[test]
    fn test_validate_k8s_name_component_rejects_underscore() {
        let result = validate_k8s_name_component("my_service", "target");
        assert!(matches!(result, Err(ValidationError::InvalidName { .. })));
        assert!(result.unwrap_err().to_string().contains("'_' not allowed"));
    }

    #[test]
    fn test_validate_k8s_name_component_rejects_leading_hyphen() {
        let result = validate_k8s_name_component("-service", "namespace");
        assert!(matches!(result, Err(ValidationError::InvalidName { .. })));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("start and end with alphanumeric")
        );
    }

    #[test]
    fn test_validate_k8s_name_component_rejects_trailing_dot() {
        let result = validate_k8s_name_component("service.", "namespace");
        assert!(matches!(result, Err(ValidationError::InvalidName { .. })));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("start and end with alphanumeric")
        );
    }

    #[test]
    fn test_load_schema_valid() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "test-key": {
                        "type": "string",
                        "default": "test",
                        "description": "Test option"
                    }
                }
            }"#,
        );

        SchemaRegistry::from_directory(temp_dir.path()).unwrap();
    }

    #[test]
    fn test_load_schema_missing_version() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "type": "object",
                "properties": {}
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains(
                    "Schema validation failed:
Error: \"version\" is a required property"
                ));
            }
            _ => panic!("Expected SchemaError for missing version"),
        }
    }

    #[test]
    fn test_unknown_namespace() {
        let temp_dir = TempDir::new().unwrap();
        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let result = registry.validate_values("unknown", &json!({}));
        assert!(matches!(result, Err(ValidationError::UnknownNamespace(..))));
    }

    #[test]
    fn test_multiple_namespaces() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "ns1",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt1": {
                        "type": "string",
                        "default": "default1",
                        "description": "First option"
                    }
                }
            }"#,
        );
        create_test_schema(
            &temp_dir,
            "ns2",
            r#"{
                "version": "2.0",
                "type": "object",
                "properties": {
                    "opt2": {
                        "type": "integer",
                        "default": 42,
                        "description": "Second option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        assert!(registry.schemas.contains_key("ns1"));
        assert!(registry.schemas.contains_key("ns2"));
    }

    #[test]
    fn test_invalid_default_type() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "bad-default": {
                        "type": "integer",
                        "default": "not-a-number",
                        "description": "A bad default value"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(
                    message.contains("Property 'bad-default': default value does not match schema")
                );
                assert!(message.contains("\"not-a-number\" is not of type \"integer\""));
            }
            _ => panic!("Expected SchemaError for invalid default type"),
        }
    }

    #[test]
    fn test_extra_properties() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "bad-property": {
                        "type": "integer",
                        "default": 0,
                        "description": "Test property",
                        "extra": "property"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(
                    message
                        .contains("Additional properties are not allowed ('extra' was unexpected)")
                );
            }
            _ => panic!("Expected SchemaError for extra properties"),
        }
    }

    #[test]
    fn test_missing_description() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "missing-desc": {
                        "type": "string",
                        "default": "test"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("\"description\" is a required property"));
            }
            _ => panic!("Expected SchemaError for missing description"),
        }
    }

    #[test]
    fn test_from_schemas_rejects_duplicate_namespace() {
        let schema_a = r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "opt": {"type": "string", "default": "a", "description": "A"}
            }
        }"#;
        let schema_b = r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "opt": {"type": "string", "default": "b", "description": "B"}
            }
        }"#;

        let result = SchemaRegistry::from_schemas(&[("test", schema_a), ("test", schema_b)]);
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("Duplicate namespace 'test'"));
            }
            _ => panic!("Expected SchemaError for duplicate namespace"),
        }
    }

    #[test]
    fn test_invalid_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        // Create a namespace directory without schema.json file
        let schema_dir = temp_dir.path().join("missing-schema");
        fs::create_dir_all(&schema_dir).unwrap();

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::FileRead(..)) => {
                // Expected error when schema.json file is missing
            }
            _ => panic!("Expected FileRead error for missing schema.json"),
        }
    }

    #[test]
    fn test_get_default() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "string_opt": {
                        "type": "string",
                        "default": "hello",
                        "description": "A string option"
                    },
                    "int_opt": {
                        "type": "integer",
                        "default": 42,
                        "description": "An integer option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let schema = registry.get("test").unwrap();

        assert_eq!(schema.get_default("string_opt"), Some(&json!("hello")));
        assert_eq!(schema.get_default("int_opt"), Some(&json!(42)));
        assert_eq!(schema.get_default("unknown"), None);
    }

    #[test]
    fn test_validate_values_valid() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable feature"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let result = registry.validate_values("test", &json!({"enabled": true}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_values_invalid_type() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "default": 0,
                        "description": "Count"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let result = registry.validate_values("test", &json!({"count": "not a number"}));
        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    #[test]
    fn test_validate_values_unknown_option() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "known_option": {
                        "type": "string",
                        "default": "default",
                        "description": "A known option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Valid known option should pass
        let result = registry.validate_values("test", &json!({"known_option": "value"}));
        assert!(result.is_ok());

        // Unknown option should fail
        let result = registry.validate_values("test", &json!({"unknown_option": "value"}));
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValueError { errors, .. }) => {
                assert!(errors.contains("Additional properties are not allowed"));
            }
            _ => panic!("Expected ValueError for unknown option"),
        }
    }

    #[test]
    fn test_object_with_additional_properties() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "scopes": {
                        "type": "object",
                        "additionalProperties": {"type": "string"},
                        "default": {},
                        "description": "A dynamic string-to-string map"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        assert!(
            registry
                .validate_values("test", &json!({"scopes": {}}))
                .is_ok()
        );
        assert!(
            registry
                .validate_values(
                    "test",
                    &json!({"scopes": {"read": "true", "write": "false"}})
                )
                .is_ok()
        );
        assert!(matches!(
            registry.validate_values("test", &json!({"scopes": {"read": 42}})),
            Err(ValidationError::ValueError { .. })
        ));
    }

    #[test]
    fn test_object_without_additional_properties_still_rejects_unknown_keys() {
        // Structured object schemas (with properties, no additionalProperties) must
        // still reject unknown keys after the fix.
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "properties": {
                            "host": {"type": "string"}
                        },
                        "default": {"host": "localhost"},
                        "description": "Server config"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Known key is valid
        let result = registry.validate_values("test", &json!({"config": {"host": "example.com"}}));
        assert!(result.is_ok());

        // Unknown key must fail
        let result = registry.validate_values(
            "test",
            &json!({"config": {"host": "example.com", "unknown": "x"}}),
        );
        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    #[test]
    fn test_object_with_fixed_properties_and_additional_properties_enforces_required() {
        // A schema that has both fixed properties and additionalProperties should still
        // enforce required on the declared fields.
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"}
                        },
                        "additionalProperties": {"type": "string"},
                        "default": {"name": "default"},
                        "description": "Config with fixed and dynamic keys"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Fixed field present, extra dynamic keys allowed
        let result =
            registry.validate_values("test", &json!({"config": {"name": "x", "extra": "y"}}));
        assert!(result.is_ok());

        // Missing required fixed field must fail
        let result = registry.validate_values("test", &json!({"config": {"extra": "y"}}));
        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    #[test]
    fn test_load_values_json_valid() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable feature"
                    },
                    "name": {
                        "type": "string",
                        "default": "default",
                        "description": "Name"
                    },
                    "count": {
                        "type": "integer",
                        "default": 0,
                        "description": "Count"
                    },
                    "rate": {
                        "type": "number",
                        "default": 0.0,
                        "description": "Rate"
                    }
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{
                "options": {
                    "enabled": true,
                    "name": "test-name",
                    "count": 42,
                    "rate": 0.75
                }
            }"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let (values, generated_at_by_namespace) = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values.len(), 1);
        assert_eq!(values["test"]["enabled"], json!(true));
        assert_eq!(values["test"]["name"], json!("test-name"));
        assert_eq!(values["test"]["count"], json!(42));
        assert_eq!(values["test"]["rate"], json!(0.75));
        assert!(generated_at_by_namespace.is_empty());
    }

    #[test]
    fn test_load_values_json_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{"version": "1.0", "type": "object", "properties": {}}"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let (values, generated_at_by_namespace) = registry
            .load_values_json(&temp_dir.path().join("nonexistent"))
            .unwrap();

        // No values.json files found, returns empty
        assert!(values.is_empty());
        assert!(generated_at_by_namespace.is_empty());
    }

    #[test]
    fn test_load_values_json_strips_unknown_keys() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "known-option": {
                        "type": "string",
                        "default": "default",
                        "description": "A known option"
                    }
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{
                "options": {
                    "known-option": "hello",
                    "unknown-option": "should be stripped"
                }
            }"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let (values, _) = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values["test"]["known-option"], json!("hello"));
        assert!(!values["test"].contains_key("unknown-option"));
    }

    #[test]
    fn test_load_values_json_skips_missing_values_file() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        // Create two schemas
        let schema_dir1 = schemas_dir.join("with-values");
        fs::create_dir_all(&schema_dir1).unwrap();
        fs::write(
            schema_dir1.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt": {"type": "string", "default": "x", "description": "Opt"}
                }
            }"#,
        )
        .unwrap();

        let schema_dir2 = schemas_dir.join("without-values");
        fs::create_dir_all(&schema_dir2).unwrap();
        fs::write(
            schema_dir2.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt": {"type": "string", "default": "x", "description": "Opt"}
                }
            }"#,
        )
        .unwrap();

        // Only create values for one namespace
        let with_values_dir = values_dir.join("with-values");
        fs::create_dir_all(&with_values_dir).unwrap();
        fs::write(
            with_values_dir.join("values.json"),
            r#"{"options": {"opt": "y"}}"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let (values, _) = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values.len(), 1);
        assert!(values.contains_key("with-values"));
        assert!(!values.contains_key("without-values"));
    }

    #[test]
    fn test_load_values_json_extracts_generated_at() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"options": {"enabled": true}, "generated_at": "2024-01-21T18:30:00.123456+00:00"}"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let (values, generated_at_by_namespace) = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values["test"]["enabled"], json!(true));
        assert_eq!(
            generated_at_by_namespace.get("test"),
            Some(&"2024-01-21T18:30:00.123456+00:00".to_string())
        );
    }

    #[test]
    fn test_propagation_event_emitted_on_generated_at_change() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"options": {"enabled": true}, "generated_at": "2024-01-21T18:30:00+00:00"}"#,
        )
        .unwrap();

        let events: Arc<Mutex<Vec<(String, f64)>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
        let store = ValuesStore::build(
            registry,
            &values_dir,
            Duration::ZERO,
            Some(Box::new(move |ns, delay| {
                events_clone.lock().unwrap().push((ns.to_string(), delay));
            })),
        )
        .unwrap();

        // Initial load doesn't emit (generated_at set during construction).
        assert!(events.lock().unwrap().is_empty());

        // Same generated_at — no event on reload.
        let _ = store.load();
        assert!(events.lock().unwrap().is_empty());

        // Update the generated_at timestamp — should emit on next load.
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"options": {"enabled": true}, "generated_at": "2024-01-21T19:00:00+00:00"}"#,
        )
        .unwrap();

        let _ = store.load();
        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "test");
        assert!(captured[0].1 > 0.0);
    }

    #[test]
    fn test_propagation_event_not_emitted_without_callback() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"options": {"enabled": true}, "generated_at": "2024-01-21T18:30:00+00:00"}"#,
        )
        .unwrap();

        let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
        // No callback — should not panic or error.
        let store = ValuesStore::build(registry, &values_dir, Duration::ZERO, None).unwrap();
        let _ = store.load();
        // Just verify it doesn't crash.
    }

    /// Write a boolean-only schema and a values file for `namespace` under
    /// `base`, returning the path of the written `values.json`.
    fn write_ns(base: &Path, namespace: &str, generated_at: &str) -> PathBuf {
        let schema_dir = base.join("schemas").join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                }
            }"#,
        )
        .unwrap();

        let ns_values_dir = base.join("values").join(namespace);
        fs::create_dir_all(&ns_values_dir).unwrap();
        let values_file = ns_values_dir.join("values.json");
        fs::write(
            &values_file,
            format!(r#"{{"options": {{"enabled": true}}, "generated_at": "{generated_at}"}}"#),
        )
        .unwrap();
        values_file
    }

    #[test]
    fn test_propagation_delay_secs_parsing() {
        let now: DateTime<Utc> = "2024-01-21T19:00:00+00:00".parse().unwrap();

        // Normal case: 30 minutes elapsed.
        assert_eq!(
            propagation_delay_secs(&now, "2024-01-21T18:30:00+00:00"),
            Some(1800.0)
        );
        // Clock skew (generated_at in the future) clamps to 0 rather than going negative.
        assert_eq!(
            propagation_delay_secs(&now, "2024-01-21T19:05:00+00:00"),
            Some(0.0)
        );
        // Malformed timestamp returns None.
        assert_eq!(propagation_delay_secs(&now, "not-a-timestamp"), None);
    }

    #[test]
    fn test_propagation_callback_panic_is_caught() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let values_file = write_ns(base, "test", "2024-01-21T18:30:00+00:00");

        let registry = Arc::new(SchemaRegistry::from_directory(&base.join("schemas")).unwrap());
        let store = ValuesStore::build(
            registry,
            &base.join("values"),
            Duration::ZERO,
            Some(Box::new(|_ns, _delay| panic!("callback boom"))),
        )
        .unwrap();

        // Trigger a change so the panicking callback fires; load() must not unwind.
        fs::write(
            &values_file,
            r#"{"options": {"enabled": false}, "generated_at": "2024-01-21T19:00:00+00:00"}"#,
        )
        .unwrap();
        let _ = store.load();

        // The values refresh still applied despite the callback panic.
        assert_eq!(
            store.values.load().get("test").unwrap().get("enabled"),
            Some(&json!(false))
        );
    }

    #[test]
    fn test_propagation_malformed_generated_at_does_not_emit() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let values_file = write_ns(base, "test", "2024-01-21T18:30:00+00:00");

        let events: Arc<Mutex<Vec<(String, f64)>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let registry = Arc::new(SchemaRegistry::from_directory(&base.join("schemas")).unwrap());
        let store = ValuesStore::build(
            registry,
            &base.join("values"),
            Duration::ZERO,
            Some(Box::new(move |ns, delay| {
                events_clone.lock().unwrap().push((ns.to_string(), delay));
            })),
        )
        .unwrap();

        // Change generated_at to a malformed value: detected as a change, but
        // unparseable, so no event is emitted.
        fs::write(
            &values_file,
            r#"{"options": {"enabled": true}, "generated_at": "garbage"}"#,
        )
        .unwrap();
        let _ = store.load();
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn test_propagation_only_changed_namespace_emits() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        write_ns(base, "alpha", "2024-01-21T18:00:00+00:00");
        let beta_values = write_ns(base, "beta", "2024-01-21T18:00:00+00:00");

        let events: Arc<Mutex<Vec<(String, f64)>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let registry = Arc::new(SchemaRegistry::from_directory(&base.join("schemas")).unwrap());
        let store = ValuesStore::build(
            registry,
            &base.join("values"),
            Duration::ZERO,
            Some(Box::new(move |ns, delay| {
                events_clone.lock().unwrap().push((ns.to_string(), delay));
            })),
        )
        .unwrap();

        // Only beta's generated_at changes; alpha is untouched.
        fs::write(
            &beta_values,
            r#"{"options": {"enabled": true}, "generated_at": "2024-01-21T19:00:00+00:00"}"#,
        )
        .unwrap();
        let _ = store.load();

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "beta");
    }

    #[test]
    fn test_load_values_json_rejects_wrong_type() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "count": {"type": "integer", "default": 0, "description": "Count"}
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"options": {"count": "not-a-number"}}"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let result = registry.load_values_json(&values_dir);

        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    mod feature_flag_tests {
        use super::*;

        const FEATURE_SCHEMA: &str = r##"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "feature.organizations:fury-mode": {
                  "$ref": "#/definitions/Feature"
                }
            }
        }"##;

        #[test]
        fn test_schema_with_valid_feature_flag() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            assert!(SchemaRegistry::from_directory(temp_dir.path()).is_ok());
        }

        #[test]
        fn test_schema_with_feature_and_regular_option() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r##"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "my-option": {
                            "type": "string",
                            "default": "hello",
                            "description": "A regular option"
                        },
                        "feature.organizations:fury-mode": {
                            "$ref": "#/definitions/Feature"
                        }
                    }
                }"##,
            );
            assert!(SchemaRegistry::from_directory(temp_dir.path()).is_ok());
        }

        #[test]
        fn test_schema_with_invalid_feature_definition() {
            let temp_dir = TempDir::new().unwrap();

            // namespace schema is invalid as feature flag is invalid.
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "feature.organizations:fury-mode": {
                            "nope": "nope"
                        }
                    }
                }"#,
            );
            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_validate_values_with_valid_feature_flag() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "owner": {"team": "hybrid-cloud"},
                        "segments": [],
                        "created_at": "2024-01-01"
                    }
                }),
            );
            assert!(result.is_ok());
        }

        #[test]
        fn test_validate_values_with_feature_flag_missing_required_field_fails() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            // Missing owner field
            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "segments": [],
                        "created_at": "2024-01-01"
                    }
                }),
            );
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_validate_values_with_feature_flag_invalid_owner_fails() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            // Owner missing required team field
            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "owner": {"email": "test@example.com"},
                        "segments": [],
                        "created_at": "2024-01-01"
                    }
                }),
            );
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_validate_values_feature_with_segments_and_conditions() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "owner": {"team": "hybrid-cloud"},
                        "enabled": true,
                        "created_at": "2024-01-01T00:00:00",
                        "segments": [
                            {
                                "name": "internal orgs",
                                "rollout": 50,
                                "conditions": [
                                    {
                                        "property": "organization_slug",
                                        "operator": "in",
                                        "value": ["sentry-test", "sentry"]
                                    }
                                ]
                            }
                        ]
                    }
                }),
            );
            assert!(result.is_ok());
        }

        #[test]
        fn test_validate_values_feature_with_multiple_condition_operators() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "owner": {"team": "hybrid-cloud"},
                        "created_at": "2024-01-01",
                        "segments": [
                            {
                                "name": "free accounts",
                                "conditions": [
                                    {
                                        "property": "subscription_is_free",
                                        "operator": "equals",
                                        "value": true
                                    }
                                ]
                            }
                        ]
                    }
                }),
            );
            assert!(result.is_ok());
        }

        #[test]
        fn test_validate_values_feature_with_invalid_condition_operator_fails() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            // Use an operator that doesn't exist
            let result = registry.validate_values(
                "test",
                &json!({
                    "feature.organizations:fury-mode": {
                        "owner": {"team": "hybrid-cloud"},
                        "created_at": "2024-01-01",
                        "segments": [
                            {
                                "name": "test segment",
                                "conditions": [
                                    {
                                        "property": "some_prop",
                                        "operator": "invalid_operator",
                                        "value": "some_value"
                                    }
                                ]
                            }
                        ]
                    }
                }),
            );
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_schema_feature_flag_not_in_options_map() {
            // Feature flags are not added to default values
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(&temp_dir, "test", FEATURE_SCHEMA);
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            let schema = registry.get("test").unwrap();

            assert!(
                schema
                    .get_default("feature.organizations:fury-mode")
                    .is_none()
            );
        }

        #[test]
        fn test_validate_values_feature_and_regular_option_together() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r##"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "my-option": {
                            "type": "string",
                            "default": "hello",
                            "description": "A regular option"
                        },
                        "feature.organizations:fury-mode": {
                            "$ref": "#/definitions/Feature"
                        }
                    }
                }"##,
            );
            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

            // Both options are valid
            let result = registry.validate_values(
                "test",
                &json!({
                    "my-option": "world",
                    "feature.organizations:fury-mode": {
                        "owner": {"team": "hybrid-cloud"},
                        "segments": [],
                        "created_at": "2024-01-01"
                    }
                }),
            );
            assert!(result.is_ok());
        }
    }

    mod store_tests {
        use super::*;

        /// Creates schema and values files for two namespaces: ns1 and ns2.
        fn setup_store_test() -> (TempDir, PathBuf, PathBuf) {
            let temp_dir = TempDir::new().unwrap();
            let schemas_dir = temp_dir.path().join("schemas");
            let values_dir = temp_dir.path().join("values");

            let ns1_schema = schemas_dir.join("ns1");
            fs::create_dir_all(&ns1_schema).unwrap();
            fs::write(
                ns1_schema.join("schema.json"),
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                    }
                }"#,
            )
            .unwrap();

            let ns1_values = values_dir.join("ns1");
            fs::create_dir_all(&ns1_values).unwrap();
            fs::write(
                ns1_values.join("values.json"),
                r#"{"options": {"enabled": true}}"#,
            )
            .unwrap();

            let ns2_schema = schemas_dir.join("ns2");
            fs::create_dir_all(&ns2_schema).unwrap();
            fs::write(
                ns2_schema.join("schema.json"),
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "count": {"type": "integer", "default": 0, "description": "Count"}
                    }
                }"#,
            )
            .unwrap();

            let ns2_values = values_dir.join("ns2");
            fs::create_dir_all(&ns2_values).unwrap();
            fs::write(
                ns2_values.join("values.json"),
                r#"{"options": {"count": 42}}"#,
            )
            .unwrap();

            (temp_dir, schemas_dir, values_dir)
        }

        fn store_with_zero_threshold(schemas_dir: &Path, values_dir: &Path) -> ValuesStore {
            let registry = Arc::new(SchemaRegistry::from_directory(schemas_dir).unwrap());
            ValuesStore::build(registry, values_dir, Duration::ZERO, None).unwrap()
        }

        #[test]
        fn test_initial_load_populates_values() {
            let (_temp, schemas_dir, values_dir) = setup_store_test();
            let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
            let store = ValuesStore::new(registry, &values_dir).unwrap();

            let guard = store.load();
            assert_eq!(guard["ns1"]["enabled"], json!(true));
            assert_eq!(guard["ns2"]["count"], json!(42));
        }

        #[test]
        fn test_read_within_threshold_serves_cached() {
            // Default 5 s threshold: a modification followed by an immediate
            // read should still see the cached value.
            let (_temp, schemas_dir, values_dir) = setup_store_test();
            let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
            let store = ValuesStore::new(registry, &values_dir).unwrap();

            // Confirm initial value, then modify the file.
            assert_eq!(store.load()["ns1"]["enabled"], json!(true));
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"options": {"enabled": false}}"#,
            )
            .unwrap();

            // Within the threshold window, the cached value is still served.
            assert_eq!(store.load()["ns1"]["enabled"], json!(true));
        }

        #[test]
        fn test_read_after_threshold_refreshes() {
            let (_temp, schemas_dir, values_dir) = setup_store_test();
            let store = store_with_zero_threshold(&schemas_dir, &values_dir);

            // Initial values.
            assert_eq!(store.load()["ns1"]["enabled"], json!(true));
            assert_eq!(store.load()["ns2"]["count"], json!(42));

            // Modify both namespaces.
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"options": {"enabled": false}}"#,
            )
            .unwrap();
            fs::write(
                values_dir.join("ns2").join("values.json"),
                r#"{"options": {"count": 100}}"#,
            )
            .unwrap();

            // With a zero threshold, the next read refreshes.
            let guard = store.load();
            assert_eq!(guard["ns1"]["enabled"], json!(false));
            assert_eq!(guard["ns2"]["count"], json!(100));
        }

        #[test]
        fn test_refresh_failure_keeps_old_values() {
            let (_temp, schemas_dir, values_dir) = setup_store_test();
            let store = store_with_zero_threshold(&schemas_dir, &values_dir);

            assert_eq!(store.load()["ns1"]["enabled"], json!(true));

            // Replace ns1 values with a type-incompatible payload.
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"options": {"enabled": "not-a-boolean"}}"#,
            )
            .unwrap();

            // Refresh attempt fails; old value still served.
            assert_eq!(store.load()["ns1"]["enabled"], json!(true));
        }

        #[test]
        fn test_concurrent_reads_observe_new_values() {
            use std::thread;

            let (_temp, schemas_dir, values_dir) = setup_store_test();
            let store = Arc::new(store_with_zero_threshold(&schemas_dir, &values_dir));

            // Prime: every thread sees the initial value.
            assert_eq!(store.load()["ns2"]["count"], json!(42));

            fs::write(
                values_dir.join("ns2").join("values.json"),
                r#"{"options": {"count": 7}}"#,
            )
            .unwrap();

            let mut handles = Vec::new();
            for _ in 0..8 {
                let store = Arc::clone(&store);
                handles.push(thread::spawn(move || {
                    let guard = store.load();
                    guard["ns2"]["count"].clone()
                }));
            }
            for h in handles {
                assert_eq!(h.join().unwrap(), json!(7));
            }
        }
    }
    mod array_tests {
        use super::*;

        #[test]
        fn test_basic_schema_validation() {
            let temp_dir = TempDir::new().unwrap();
            for (a_type, default) in [
                ("boolean", ""), // empty array test
                ("boolean", "true"),
                ("integer", "1"),
                ("number", "1.2"),
                ("string", "\"wow\""),
            ] {
                create_test_schema(
                    &temp_dir,
                    "test",
                    &format!(
                        r#"{{
                        "version": "1.0",
                        "type": "object",
                        "properties": {{
                            "array-key": {{
                                "type": "array",
                                "items": {{"type": "{}"}},
                                "default": [{}],
                                "description": "Array option"
                                }}
                            }}
                        }}"#,
                        a_type, default
                    ),
                );

                SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            }
        }

        #[test]
        fn test_missing_items_object_rejection() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "default": [1,2,3],
                            "description": "Array option"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
        }

        #[test]
        fn test_malformed_items_rejection() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": ""},
                            "default": [1,2,3],
                            "description": "Array option"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
        }

        #[test]
        fn test_schema_default_type_mismatch_rejection() {
            let temp_dir = TempDir::new().unwrap();
            // also tests real number rejection when type is integer
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "default": [1,2,3.3],
                            "description": "Array option"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
        }

        #[test]
        fn test_schema_default_heterogeneous_rejection() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "default": [1,2,"uh oh!"],
                            "description": "Array option"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
        }

        #[test]
        fn test_load_values_valid() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "default": [1,2,3],
                            "description": "Array option"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "array-key": [4,5,6]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let (values, generated_at_by_namespace) =
                registry.load_values_json(&values_dir).unwrap();

            assert_eq!(values.len(), 1);
            assert_eq!(values["test"]["array-key"], json!([4, 5, 6]));
            assert!(generated_at_by_namespace.is_empty());
        }

        #[test]
        fn test_reject_values_not_an_array() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "default": [1,2,3],
                            "description": "Array option"
                        }
                    }
                }"#,
                // sneaky! not an array
                r#"{
                    "options": {
                        "array-key": "[]"
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);

            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_reject_values_mismatch() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "array-key": {
                            "type": "array",
                            "items": {"type": "integer"},
                            "default": [1,2,3],
                            "description": "Array option"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "array-key": ["a","b","c"]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);

            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }
    }

    mod object_tests {
        use super::*;

        #[test]
        fn test_object_schema_loads() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"},
                                "rate": {"type": "number"},
                                "enabled": {"type": "boolean"}
                            },
                            "default": {"host": "localhost", "port": 8080, "rate": 0.5, "enabled": true},
                            "description": "Service config"
                        }
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            let schema = registry.get("test").unwrap();
            assert_eq!(schema.options["config"].option_type, "object");
        }

        #[test]
        fn test_object_missing_properties_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "default": {"host": "localhost"},
                            "description": "Missing properties field"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_default_wrong_type_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"}
                            },
                            "default": {"host": "localhost", "port": "not-a-number"},
                            "description": "Bad default"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_default_missing_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"}
                            },
                            "default": {"host": "localhost"},
                            "description": "Missing port in default"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_default_extra_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"}
                            },
                            "default": {"host": "localhost", "extra": "field"},
                            "description": "Extra field in default"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_values_valid() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"}
                            },
                            "default": {"host": "localhost", "port": 8080},
                            "description": "Service config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com", "port": 9090}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(result.is_ok());
        }

        #[test]
        fn test_object_values_wrong_field_type_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"}
                            },
                            "default": {"host": "localhost", "port": 8080},
                            "description": "Service config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com", "port": "not-a-number"}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_object_values_extra_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"}
                            },
                            "default": {"host": "localhost"},
                            "description": "Service config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com", "extra": "field"}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_object_values_missing_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"}
                            },
                            "default": {"host": "localhost", "port": 8080},
                            "description": "Service config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com"}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        // Array of objects tests

        #[test]
        fn test_array_of_objects_schema_loads() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [{"url": "https://a.example.com", "weight": 1}],
                            "description": "Endpoints"
                        }
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            let schema = registry.get("test").unwrap();
            assert_eq!(schema.options["endpoints"].option_type, "array");
        }

        #[test]
        fn test_array_of_objects_empty_default() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            assert!(registry.get("test").is_some());
        }

        #[test]
        fn test_array_of_objects_default_wrong_field_type_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [{"url": "https://a.example.com", "weight": "not-a-number"}],
                            "description": "Endpoints"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_array_of_objects_missing_items_properties_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object"
                            },
                            "default": [],
                            "description": "Missing properties in items"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_array_of_objects_values_valid() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "endpoints": [
                            {"url": "https://a.example.com", "weight": 1},
                            {"url": "https://b.example.com", "weight": 2}
                        ]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(result.is_ok());
        }

        #[test]
        fn test_array_of_objects_values_wrong_item_shape_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "endpoints": [
                            {"url": "https://a.example.com", "weight": "not-a-number"}
                        ]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_array_of_objects_values_extra_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "endpoints": [
                            {"url": "https://a.example.com", "extra": "field"}
                        ]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        #[test]
        fn test_array_of_objects_values_missing_field_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer"}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "endpoints": [
                            {"url": "https://a.example.com"}
                        ]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(matches!(result, Err(ValidationError::ValueError { .. })));
        }

        // Optional field tests

        #[test]
        fn test_object_optional_field_can_be_omitted_from_default() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"host": "localhost"},
                            "description": "Config with optional field"
                        }
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            let schema = registry.get("test").unwrap();
            assert_eq!(schema.options["config"].option_type, "object");
        }

        #[test]
        fn test_object_optional_field_can_be_included_in_default() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"host": "localhost", "debug": true},
                            "description": "Config with optional field included"
                        }
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
            assert!(registry.get("test").is_some());
        }

        #[test]
        fn test_object_optional_field_wrong_type_rejected() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"host": "localhost", "debug": "not-a-bool"},
                            "description": "Optional field wrong type"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_required_field_still_required_with_optional_present() {
            let temp_dir = TempDir::new().unwrap();
            create_test_schema(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "port": {"type": "integer"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"debug": true},
                            "description": "Missing required fields"
                        }
                    }
                }"#,
            );

            let result = SchemaRegistry::from_directory(temp_dir.path());
            assert!(result.is_err());
        }

        #[test]
        fn test_object_optional_field_omitted_from_values() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"host": "localhost"},
                            "description": "Config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com"}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(result.is_ok());
        }

        #[test]
        fn test_object_optional_field_included_in_values() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "config": {
                            "type": "object",
                            "properties": {
                                "host": {"type": "string"},
                                "debug": {"type": "boolean", "optional": true}
                            },
                            "default": {"host": "localhost"},
                            "description": "Config"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "config": {"host": "example.com", "debug": true}
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(result.is_ok());
        }

        #[test]
        fn test_array_of_objects_optional_field_omitted() {
            let temp_dir = TempDir::new().unwrap();
            let (schemas_dir, values_dir) = create_test_schema_with_values(
                &temp_dir,
                "test",
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "endpoints": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string"},
                                    "weight": {"type": "integer", "optional": true}
                                }
                            },
                            "default": [],
                            "description": "Endpoints"
                        }
                    }
                }"#,
                r#"{
                    "options": {
                        "endpoints": [
                            {"url": "https://a.example.com"},
                            {"url": "https://b.example.com", "weight": 2}
                        ]
                    }
                }"#,
            );

            let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
            let result = registry.load_values_json(&values_dir);
            assert!(result.is_ok());
        }
    }
}
