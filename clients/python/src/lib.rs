//! Python bindings for sentry-options using PyO3.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use ::sentry_options::{
    DEFAULT_REFRESH_THRESHOLD, FeatureChecker as RustFeatureChecker,
    FeatureContext as RustFeatureContext, Options as RustOptions, OptionsError as RustOptionsError,
};
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use serde_json::Value;

// Global options instance
static GLOBAL_OPTIONS: OnceLock<RustOptions> = OnceLock::new();

// Custom exception hierarchy
pyo3::create_exception!(
    sentry_options,
    OptionsError,
    PyException,
    "Base exception for sentry-options errors."
);
pyo3::create_exception!(
    sentry_options,
    SchemaError,
    OptionsError,
    "Raised when schema loading or validation fails."
);
pyo3::create_exception!(
    sentry_options,
    UnknownNamespaceError,
    OptionsError,
    "Raised when accessing an unknown namespace."
);
pyo3::create_exception!(
    sentry_options,
    UnknownOptionError,
    OptionsError,
    "Raised when accessing an unknown option."
);
pyo3::create_exception!(
    sentry_options,
    NotInitializedError,
    OptionsError,
    "Raised when options have not been initialized."
);

/// Convert serde_json::Value to Python object.
fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any().unbind()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PyInt::new(py, i).into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(PyFloat::new(py, f).into_any().unbind())
            } else {
                Err(PyValueError::new_err("Invalid number"))
            }
        }
        Value::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                let py_item = json_to_py(py, item)?; // parse child recursively
                list.append(py_item)?;
            }
            Ok(list.into_any().unbind())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_val = json_to_py(py, v)?;
                dict.set_item(k, py_val)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert Python object to serde_json::Value.
fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        Ok(Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::Number(i.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::Number(serde_json::Number::from_f64(f).ok_or_else(
            || PyValueError::new_err("Cannot convert NaN or Infinity to JSON"),
        )?))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(Value::String(s))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let items: Result<Vec<Value>, _> = list.iter().map(|item| py_to_json(&item)).collect();
        Ok(Value::Array(items?))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key: String = k
                .extract()
                .map_err(|_| PyValueError::new_err("Dict keys must be strings"))?;
            map.insert(key, py_to_json(&v)?);
        }
        Ok(Value::Object(map))
    } else {
        Err(PyValueError::new_err("Unsupported type for override"))
    }
}

fn options_err(err: RustOptionsError) -> PyErr {
    match err {
        RustOptionsError::NotInitialized => {
            NotInitializedError::new_err("Options not initialized - call init() first")
        }
        RustOptionsError::AlreadyInitialized => {
            PyRuntimeError::new_err("Options already initialized")
        }
        RustOptionsError::UnknownNamespace(ns) => {
            UnknownNamespaceError::new_err(format!("Unknown namespace: {}", ns))
        }
        RustOptionsError::UnknownOption { namespace, key } => UnknownOptionError::new_err(format!(
            "Unknown option '{}' in namespace '{}'",
            key, namespace
        )),
        RustOptionsError::Schema(e) => SchemaError::new_err(e.to_string()),
    }
}

/// Feature evaluation context holding arbitrary key-value data.
///
/// Pass a dict of context data and optional identity_fields to control
/// rollout bucketing. Identity fields are sorted and hashed to produce
/// a stable identifier used for percentage rollouts.
#[pyclass(name = "FeatureContext", unsendable)]
struct PyFeatureContext {
    inner: RustFeatureContext,
}

#[pymethods]
impl PyFeatureContext {
    #[new]
    #[pyo3(signature = (data, *, identity_fields=None))]
    fn new(
        data: HashMap<String, Py<PyAny>>,
        identity_fields: Option<Vec<String>>,
        py: Python<'_>,
    ) -> PyResult<Self> {
        let mut ctx = RustFeatureContext::new();
        for (key, val) in &data {
            let json_val = py_to_json(val.bind(py))?;
            ctx.insert(key, json_val);
        }
        if let Some(fields) = identity_fields {
            let field_refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            ctx.identity_fields(field_refs);
        }
        Ok(PyFeatureContext { inner: ctx })
    }

    fn __repr__(&self) -> String {
        "FeatureContext(...)".to_string()
    }
}

/// Handle for evaluating feature flags within a specific namespace.
#[pyclass(name = "FeatureChecker")]
struct PyFeatureChecker {
    inner: RustFeatureChecker,
}

#[pymethods]
impl PyFeatureChecker {
    /// Check whether a feature flag is enabled for a given context.
    ///
    /// Returns false if the feature is not defined, not enabled, or conditions don't match.
    fn has(&self, feature_name: &str, context: PyRef<'_, PyFeatureContext>) -> bool {
        self.inner.has(feature_name, &context.inner)
    }

    fn __repr__(&self) -> String {
        "FeatureChecker(...)".to_string()
    }
}

/// Initialize global options using fallback chain: SENTRY_OPTIONS_DIR env var,
/// then /etc/sentry-options if it exists, otherwise sentry-options/.
///
/// Optionally accepts an `on_propagation` callback that fires whenever values
/// are refreshed with a new `generated_at` timestamp. The callback receives
/// `(namespace: str, delay_secs: float)`.
///
/// `refresh_threshold` is the staleness threshold in seconds for
/// refresh-on-read (default: 5.0). Pass `None` to disable refresh-on-read
/// entirely; values then only change via `refresh()`.
///
/// `additional_schemas` adds namespace schemas from memory as a
/// `{namespace: schema_json}` mapping, alongside those read from `{dir}/schemas/`,
/// useful for schemas only known at runtime. Errors on a namespace already on
/// disk. Values still load from disk.
#[pyfunction]
#[pyo3(signature = (on_propagation=None, refresh_threshold=Some(DEFAULT_REFRESH_THRESHOLD.as_secs_f64()), additional_schemas=None))]
fn init(
    on_propagation: Option<Py<PyAny>>,
    refresh_threshold: Option<f64>,
    additional_schemas: Option<HashMap<String, String>>,
) -> PyResult<()> {
    if GLOBAL_OPTIONS.get().is_some() {
        return Ok(());
    }

    let refresh_threshold = match refresh_threshold {
        Some(secs) if !secs.is_finite() || secs < 0.0 => {
            return Err(PyValueError::new_err(
                "refresh_threshold must be a non-negative number or None",
            ));
        }
        Some(secs) => Some(Duration::from_secs_f64(secs)),
        None => None,
    };

    // we need to own the ones passed in
    let owned_schemas: Vec<(String, String)> =
        additional_schemas.unwrap_or_default().into_iter().collect();
    let schema_refs: Vec<(&str, &str)> = owned_schemas
        .iter()
        .map(|(ns, json)| (ns.as_str(), json.as_str()))
        .collect();

    let mut builder = RustOptions::builder().with_refresh_threshold(refresh_threshold);
    if !schema_refs.is_empty() {
        builder = builder.with_additional_schemas(&schema_refs);
    }
    if let Some(cb) = on_propagation {
        // `Py<PyAny>` is `Send + Sync`; the closure re-acquires the GIL for the
        // call, so the callback can run on whichever thread triggers a refresh.
        builder = builder.with_callback(move |ns, delay| {
            Python::attach(|py| {
                if let Err(e) = cb.call1(py, (ns, delay)) {
                    e.print(py);
                }
            });
        });
    }
    let opts = builder.build().map_err(options_err)?;
    let _ = GLOBAL_OPTIONS.set(opts);
    Ok(())
}

/// Refresh values from disk, ignoring the staleness threshold.
///
/// Returns whether a new snapshot was published, i.e. the files changed on
/// disk and reloaded successfully. On error the previous snapshot is retained
/// and served. Calling this more often than the threshold guarantees reads
/// never refresh inline.
///
/// Raises NotInitializedError if init() has not been called.
#[pyfunction]
fn refresh(py: Python<'_>) -> PyResult<bool> {
    let opts = GLOBAL_OPTIONS.get().ok_or_else(|| {
        NotInitializedError::new_err("Options not initialized - call init() first")
    })?;
    // Release the GIL: the reload does file IO, and the propagation callback
    // re-acquires the GIL itself.
    py.detach(|| opts.refresh()).map_err(options_err)
}

/// Get a namespace handle for accessing options.
///
/// Raises RuntimeError if init() has not been called.
#[pyfunction]
fn options(namespace: String) -> PyResult<NamespaceOptions> {
    let opts = GLOBAL_OPTIONS.get().ok_or_else(|| {
        NotInitializedError::new_err("Options not initialized - call init() first")
    })?;
    Ok(NamespaceOptions {
        namespace,
        options: opts,
    })
}

/// Get a feature checker for a namespace.
///
/// Raises RuntimeError if init() has not been called.
#[pyfunction]
fn features(namespace: String) -> PyResult<PyFeatureChecker> {
    let opts = GLOBAL_OPTIONS.get().ok_or_else(|| {
        NotInitializedError::new_err("Options not initialized - call init() first")
    })?;
    Ok(PyFeatureChecker {
        inner: RustFeatureChecker::new(namespace, opts),
    })
}

/// Handle for accessing options within a specific namespace.
#[pyclass]
struct NamespaceOptions {
    namespace: String,
    options: &'static RustOptions,
}

#[pymethods]
impl NamespaceOptions {
    /// Get an option value, returning the schema default if not set.
    fn get(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let value = self
            .options
            .get(&self.namespace, key)
            .map_err(options_err)?;
        json_to_py(py, &value)
    }

    /// Like `get`, but always refreshes. Refresh incurs a cost so this should
    /// only be used in testing.
    fn get_forced(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let value = self
            .options
            .get_forced(&self.namespace, key)
            .map_err(options_err)?;
        json_to_py(py, &value)
    }

    /// Check if an option has a defined value.
    fn isset(&self, key: &str) -> PyResult<bool> {
        self.options
            .isset(&self.namespace, key)
            .map_err(options_err)
    }

    fn __repr__(&self) -> String {
        format!("NamespaceOptions({:?})", self.namespace)
    }
}

// Testing utilities - override storage and validation

/// Set a thread-local override. Returns the previous value (as a Python object) or None.
#[pyfunction]
fn _set_override(
    py: Python<'_>,
    namespace: String,
    key: String,
    value: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let json_value = py_to_json(value)?;
    let previous = ::sentry_options::testing::get_override(&namespace, &key);
    ::sentry_options::testing::set_override(&namespace, &key, json_value);
    match previous {
        Some(v) => json_to_py(py, &v),
        None => Ok(py.None()),
    }
}

/// Clear a thread-local override.
#[pyfunction]
fn _clear_override(namespace: String, key: String) {
    ::sentry_options::testing::clear_override(&namespace, &key);
}

/// Validate an option value against the schema (used by testing.py).
#[pyfunction]
fn _validate_option(namespace: String, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
    let json_value = py_to_json(value)?;
    let opts = GLOBAL_OPTIONS.get().ok_or_else(|| {
        NotInitializedError::new_err("Options not initialized - call init() first")
    })?;
    opts.validate_override(&namespace, &key, &json_value)
        .map_err(options_err)?;
    Ok(())
}

/// Python module definition.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Functions
    m.add_function(wrap_pyfunction!(init, m)?)?;
    m.add_function(wrap_pyfunction!(options, m)?)?;
    m.add_function(wrap_pyfunction!(features, m)?)?;
    m.add_function(wrap_pyfunction!(refresh, m)?)?;
    // Classes
    m.add_class::<NamespaceOptions>()?;
    m.add_class::<PyFeatureContext>()?;
    m.add_class::<PyFeatureChecker>()?;
    // Exceptions
    m.add("OptionsError", m.py().get_type::<OptionsError>())?;
    m.add("SchemaError", m.py().get_type::<SchemaError>())?;
    m.add(
        "UnknownNamespaceError",
        m.py().get_type::<UnknownNamespaceError>(),
    )?;
    m.add(
        "UnknownOptionError",
        m.py().get_type::<UnknownOptionError>(),
    )?;
    m.add(
        "NotInitializedError",
        m.py().get_type::<NotInitializedError>(),
    )?;
    // Testing utilities (only called by testing.py)
    m.add_function(wrap_pyfunction!(_set_override, m)?)?;
    m.add_function(wrap_pyfunction!(_clear_override, m)?)?;
    m.add_function(wrap_pyfunction!(_validate_option, m)?)?;
    Ok(())
}
