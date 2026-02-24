mod loader;
mod option_usage;
mod output;
mod repo_schema_config;
mod schema_evolution;
mod schema_retriever;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::{Args, Parser, Subcommand};
use sentry_options_validation::{LOCAL_OPTIONS_DIR, OPTIONS_DIR_ENV, SchemaRegistry};

use loader::{ensure_no_duplicate_keys, load_and_validate};
use option_usage::check_option_usage;
use output::{OutputFormat, generate_configmap, generate_json, write_configmap_yaml, write_json};

/// Result type for operations
pub type Result<T> = std::result::Result<T, AppError>;

/// Errors that can occur during option processing
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),

    #[error("Duplicate key '{key}' found in {first_file} and {second_file}")]
    DuplicateKey {
        key: String,
        first_file: String,
        second_file: String,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error in {path}: {source}")]
    YamlParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Directory walk error: {0}")]
    Walk(#[from] walkdir::Error),

    #[error("Schema validation error: {0}")]
    Schema(#[from] sentry_options_validation::ValidationError),

    #[error("Git command failed: {0}")]
    Git(String),
}

/// defines the CLI for sentry-options validation and processing
#[derive(Parser, Debug)]
#[command(name = "sentry-options-cli")]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, global = true, help = "suppress output messages")]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

/// Arguments for the write command
#[derive(Args, Debug)]
struct WriteArgs {
    #[arg(
        long,
        required = true,
        help = "directory containing namespace schema definitions"
    )]
    schemas: String,

    #[arg(long, required = true, help = "root directory of the sentry options")]
    root: String,

    #[arg(long, help = "output path (directory for json, file for configmap)")]
    out: Option<String>,

    #[arg(
        long,
        value_enum,
        default_value = "json",
        help = "output format: json (files) or configmap (stdout YAML)"
    )]
    output_format: OutputFormat,

    #[arg(
        long,
        help = "git commit SHA for traceability (used in configmap annotations)"
    )]
    commit_sha: Option<String>,

    #[arg(
        long,
        help = "git commit timestamp for SLO tracking (used in configmap annotations)"
    )]
    commit_timestamp: Option<String>,

    #[arg(
        long,
        help = "namespace to generate ConfigMap for (required for configmap format)"
    )]
    namespace: Option<String>,

    #[arg(
        long,
        help = "target to generate ConfigMap for (required for configmap format)"
    )]
    target: Option<String>,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate schema definitions in a directory
    #[command(name = "validate-schema")]
    ValidateSchema {
        #[arg(
            long,
            required = true,
            help = "directory containing namespace schema definitions"
        )]
        schemas: String,
    },
    /// Validate option values against schemas
    #[command(name = "validate-values")]
    ValidateValues {
        #[arg(
            long,
            required = true,
            help = "directory containing namespace schema definitions"
        )]
        schemas: String,

        #[arg(long, required = true, help = "root directory of the sentry options")]
        root: String,
    },
    /// Validate and convert YAML values to JSON or ConfigMap
    Write(WriteArgs),
    /// Fetch schemas from multiple repos via git sparse checkout
    #[command(name = "fetch-schemas")]
    FetchSchemas {
        #[arg(long, required = true, help = "path to repos.json config")]
        config: String,

        #[arg(long, required = true, help = "output directory for schemas")]
        out: String,
    },
    /// Validate schema changes between two git SHAs
    #[command(name = "validate-schema-changes")]
    ValidateSchemaChanges {
        #[arg(long, required = true, help = "base commit SHA to compare from")]
        base_sha: String,

        #[arg(long, required = true, help = "head commit SHA to compare to")]
        head_sha: String,

        #[arg(
            long,
            required = true,
            help = "repository name for namespace prefix validation"
        )]
        repo: String,

        #[arg(long, help = "output the deleted options as a space-separated string")]
        report_deletions: bool,
    },
    #[command(name = "check-option-usage")]
    CheckOptionUsage {
        #[arg(
            long,
            required = true,
            help = "space-separated colon delimited list of namespace:option pairs"
        )]
        deletions: String,

        #[arg(long, required = true, help = "root directory of the sentry options")]
        root: String,
    },
}

/// A key value pair of options and their parsed value
pub type OptionsMap = HashMap<String, serde_json::Value>;

/// Represents a filepath and its parsed YAML data
#[derive(Debug, PartialEq, Eq)]
pub struct FileData {
    pub path: String,
    pub data: OptionsMap,
}

impl Ord for FileData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

impl PartialOrd for FileData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A map representation of an option namespace
/// outer map is keyed by namespace
/// inner map is keyed by target, value a list of files
pub type NamespaceMap = HashMap<String, HashMap<String, Vec<FileData>>>;

fn cli_validate_schema(schemas: String, quiet: bool) -> Result<()> {
    SchemaRegistry::from_directory(Path::new(&schemas))?;

    if !quiet {
        println!("Schema validation successful");
    }
    Ok(())
}

fn cli_validate_values(schemas: String, root: String, quiet: bool) -> Result<()> {
    let schema_registry = SchemaRegistry::from_directory(Path::new(&schemas))?;
    let grouped = load_and_validate(&root, &schema_registry)?;
    ensure_no_duplicate_keys(&grouped)?;

    if !quiet {
        println!("Values validation successful");
    }
    Ok(())
}

fn cli_write(args: WriteArgs, quiet: bool) -> Result<()> {
    let schema_registry = SchemaRegistry::from_directory(Path::new(&args.schemas))?;

    let grouped = load_and_validate(&args.root, &schema_registry)?;
    ensure_no_duplicate_keys(&grouped)?;

    let generated_at = chrono::Utc::now().to_rfc3339();

    match args.output_format {
        OutputFormat::Json => {
            let out_path = args.out.ok_or_else(|| {
                AppError::Validation("--out is required for json output format".to_string())
            })?;
            let json_outputs = generate_json(grouped, &generated_at)?;
            let num_files = json_outputs.len();
            write_json(PathBuf::from(&out_path), json_outputs)?;

            if !quiet {
                println!("Successfully wrote {} output files", num_files);
            }
        }
        OutputFormat::Configmap => {
            let namespace = args.namespace.ok_or_else(|| {
                AppError::Validation("--namespace is required for configmap output format".into())
            })?;
            let target = args.target.ok_or_else(|| {
                AppError::Validation("--target is required for configmap output format".into())
            })?;
            let configmap = generate_configmap(
                &grouped,
                &namespace,
                &target,
                args.commit_sha.as_deref(),
                args.commit_timestamp.as_deref(),
                &generated_at,
            )?;

            let out_path = args.out.as_ref().map(Path::new);
            write_configmap_yaml(&configmap, out_path)?;

            if !quiet {
                match out_path {
                    Some(path) => eprintln!("Successfully wrote ConfigMap to {}", path.display()),
                    None => eprintln!(
                        "Successfully generated ConfigMap: sentry-options-{}",
                        namespace
                    ),
                }
            }
        }
    }
    Ok(())
}

fn cli_fetch_schemas(config: String, out: String, quiet: bool) -> Result<()> {
    let config = repo_schema_config::RepoSchemaConfigs::from_file(Path::new(&config))?;
    schema_retriever::fetch_all_schemas(&config, Path::new(&out), quiet)?;
    if !quiet {
        println!("Successfully fetched schemas to {}", out);
    }
    Ok(())
}

fn cli_validate_schema_changes(
    base_sha: String,
    head_sha: String,
    repo: String,
    report_deletions: bool,
    quiet: bool,
) -> Result<()> {
    let base_temp = tempfile::tempdir()?;
    let head_temp = tempfile::tempdir()?;

    // Get schemas path from env or use default
    let schemas_path =
        std::env::var(OPTIONS_DIR_ENV).unwrap_or_else(|_| format!("{}/schemas", LOCAL_OPTIONS_DIR));

    // for git archive to work we need to ensure shas are pre-fetched
    schema_retriever::fetch_shas(&[&base_sha, &head_sha])?;

    schema_retriever::extract_schemas_at_sha(&base_sha, &schemas_path, base_temp.path())?;
    schema_retriever::extract_schemas_at_sha(&head_sha, &schemas_path, head_temp.path())?;

    let base_extracted = base_temp.path().join(&schemas_path);
    let head_extracted = head_temp.path().join(&schemas_path);

    schema_evolution::detect_changes(
        &base_extracted,
        &head_extracted,
        &repo,
        report_deletions,
        quiet,
    )?;

    if !quiet {
        eprintln!("Schema validation passed");
    }

    Ok(())
}

fn cli_check_option_usage(deletions: String, root: String) -> Result<()> {
    let res = check_option_usage(deletions, Path::new(&root))?;
    println!("{}", res);
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::ValidateSchema { schemas } => cli_validate_schema(schemas, cli.quiet),
        Commands::ValidateValues { schemas, root } => cli_validate_values(schemas, root, cli.quiet),
        Commands::Write(args) => cli_write(args, cli.quiet),
        Commands::FetchSchemas { config, out } => cli_fetch_schemas(config, out, cli.quiet),
        Commands::ValidateSchemaChanges {
            base_sha,
            head_sha,
            repo,
            report_deletions,
        } => cli_validate_schema_changes(base_sha, head_sha, repo, report_deletions, cli.quiet),
        Commands::CheckOptionUsage { deletions, root } => cli_check_option_usage(deletions, root),
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    /// Test fixture that manages temp directories and schema registry
    struct TestFixture {
        options_dir: TempDir,
        registry: SchemaRegistry,
    }

    impl TestFixture {
        /// create a new test fixture with test schemas for the given namespaces
        /// Each namespace gets a schema with the 4 test properties: string_val, int_val, float_val, bool_val
        fn new(namespaces: &[&str]) -> Self {
            let options_dir = TempDir::new().unwrap();
            let schema_dir = TempDir::new().unwrap();

            for ns in namespaces {
                let ns_dir = schema_dir.path().join(ns);
                fs::create_dir_all(&ns_dir).unwrap();
                let schema_content = r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "string_val": {"type": "string", "default": "", "description": "test"},
                        "int_val": {"type": "integer", "default": 0, "description": "test"},
                        "float_val": {"type": "number", "default": 0.0, "description": "test"},
                        "bool_val": {"type": "boolean", "default": false, "description": "test"}
                    }
                }"#;
                fs::write(ns_dir.join("schema.json"), schema_content).unwrap();
            }

            let registry = SchemaRegistry::from_directory(schema_dir.path()).unwrap();
            Self {
                options_dir,
                registry,
            }
        }

        /// create a file in the options directory
        fn create_file(&self, namespace: &str, target: &str, filename: &str, content: &str) {
            let dir = self.options_dir.path().join(namespace).join(target);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(filename), content).unwrap();
        }

        /// helper to call load_and_validate with the given options_dir and registry
        fn load(&self) -> Result<NamespaceMap> {
            load_and_validate(self.options_dir.path().to_str().unwrap(), &self.registry)
        }
    }

    /// helper function to build a yaml file
    fn valid_yaml(options: &[(&str, &str)]) -> String {
        let mut yaml = String::from("options:\n");
        for (key, value) in options {
            yaml.push_str(&format!("  {}: {}\n", key, value));
        }
        yaml
    }

    #[test]
    fn test_load_nonexistent_directory() {
        let registry = SchemaRegistry::new();
        let result = load_and_validate("/foo/bar/baz", &registry);
        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::Walk(_))));
    }

    #[test]
    fn test_invalid_directory_structure_too_few_levels() {
        let f = TestFixture::new(&[]);
        let path = f.options_dir.path().join("options.yaml");
        fs::write(&path, "options:\n  key: value").unwrap();

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("Invalid directory structure"));
                assert!(msg.contains("expected namespace/target/file.yaml"));
            }
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_invalid_directory_structure_too_many_levels() {
        let f = TestFixture::new(&[]);
        let deep_dir = f
            .options_dir
            .path()
            .join("namespace")
            .join("target")
            .join("extra_level")
            .join("level");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(deep_dir.join("file.yaml"), "options:\n  key: value").unwrap();

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("Invalid directory structure"));
            }
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_yml_extension_rejected() {
        let f = TestFixture::new(&["test"]);
        f.create_file("test", "default", "bad.yml", "options:\n  key: value");

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected .yaml, found .yml"));
            }
            _ => panic!("Expected Validation error for .yml extension"),
        }
    }

    #[test]
    fn test_non_yaml_files_ignored() {
        let f = TestFixture::new(&["test"]);
        f.create_file("test", "default", "README.md", "# Documentation");
        f.create_file("test", "default", "config.txt", "some text");
        f.create_file(
            "test",
            "default",
            "valid.yaml",
            &valid_yaml(&[("string_val", "\"value\"")]),
        );

        let result = f.load();
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(
            grouped.get("test").unwrap().get("default").unwrap().len(),
            1
        );
    }

    #[test]
    fn test_empty_yaml_file() {
        let f = TestFixture::new(&["test"]);
        f.create_file("test", "default", "empty.yaml", "");

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::YamlParse { .. }) | Err(AppError::Validation(_)) => {}
            _ => panic!("Expected YAML parse or validation error for empty file"),
        }
    }

    #[test]
    fn test_invalid_yaml_syntax() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "bad.yaml",
            "options:\n  key: [\n  invalid",
        );

        let result = f.load();
        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::YamlParse { .. })));
    }

    #[test]
    fn test_yaml_missing_options_key() {
        let f = TestFixture::new(&["test"]);
        f.create_file("test", "default", "bad.yaml", "settings:\n  key: value");

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected"));
                assert!(msg.contains("options"));
            }
            _ => panic!("Expected Validation error for missing 'options' key"),
        }
    }

    #[test]
    fn test_yaml_multiple_top_level_keys() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "bad.yaml",
            "options:\n  key: value\nextra:\n  other: value",
        );

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("exactly one top level key"));
            }
            _ => panic!("Expected Validation error for multiple top-level keys"),
        }
    }

    #[test]
    fn test_options_not_a_mapping() {
        let f = TestFixture::new(&["test"]);
        f.create_file("test", "default", "bad.yaml", "options: 12345");

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected 'options' to be a mapping"));
            }
            _ => panic!("Expected Validation error when options is not a mapping"),
        }
    }

    #[test]
    fn test_valid_single_namespace() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[("string_val", "\"value1\""), ("int_val", "42")]),
        );

        let result = f.load();
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert!(grouped.contains_key("test"));
        assert!(grouped.get("test").unwrap().contains_key("default"));
    }

    #[test]
    fn test_valid_multiple_namespaces() {
        let f = TestFixture::new(&["ns1", "ns2"]);
        f.create_file(
            "ns1",
            "default",
            "base.yaml",
            &valid_yaml(&[("string_val", "\"value1\"")]),
        );
        f.create_file(
            "ns2",
            "default",
            "base.yaml",
            &valid_yaml(&[("int_val", "42")]),
        );

        let result = f.load();
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key("ns1"));
        assert!(grouped.contains_key("ns2"));
    }

    #[test]
    fn test_duplicate_keys_in_same_target() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "file1.yaml",
            &valid_yaml(&[("string_val", "\"value1\"")]),
        );
        f.create_file(
            "test",
            "default",
            "file2.yaml",
            &valid_yaml(&[("string_val", "\"value2\"")]),
        );

        let grouped = f.load().unwrap();
        let result = ensure_no_duplicate_keys(&grouped);
        assert!(result.is_err());
        match result {
            Err(AppError::DuplicateKey { key, .. }) => {
                assert_eq!(key, "string_val");
            }
            _ => panic!("Expected DuplicateKey error"),
        }
    }

    #[test]
    fn test_namespace_missing_default_target() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "s4s",
            "base.yaml",
            &valid_yaml(&[("string_val", "\"value1\"")]),
        );

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("missing required 'default' target"));
            }
            _ => panic!("Expected Validation error for missing default target"),
        }
    }

    #[test]
    fn test_multiple_files_in_target() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "file1.yaml",
            &valid_yaml(&[("string_val", "\"value1\"")]),
        );
        f.create_file(
            "test",
            "default",
            "file2.yaml",
            &valid_yaml(&[("int_val", "42")]),
        );

        let result = f.load();
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(
            grouped.get("test").unwrap().get("default").unwrap().len(),
            2
        );
    }

    #[test]
    fn test_target_override() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[("string_val", "\"default_value\""), ("int_val", "100")]),
        );
        f.create_file(
            "test",
            "s4s",
            "override.yaml",
            &valid_yaml(&[("string_val", "\"overridden\"")]),
        );

        let grouped = f.load().unwrap();
        let json_outputs = generate_json(grouped, "2026-01-21T00:00:00Z").unwrap();

        // Find the s4s output
        let s4s_output = json_outputs
            .iter()
            .find(|(name, _)| name.contains("s4s"))
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(&s4s_output.1).unwrap();

        // Check that string_val was overridden
        assert_eq!(
            json["options"]["string_val"].as_str().unwrap(),
            "overridden"
        );
        // Check that int_val still has default value
        assert_eq!(json["options"]["int_val"].as_i64().unwrap(), 100);
    }

    #[test]
    fn test_output_keys_sorted_alphabetically() {
        let f = TestFixture::new(&["test"]);
        // Insert in non-alphabetical order to verify sorting
        f.create_file(
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[
                ("string_val", "\"z\""),
                ("bool_val", "true"),
                ("int_val", "1"),
            ]),
        );

        let grouped = f.load().unwrap();
        let json_outputs = generate_json(grouped, "2026-01-21T00:00:00Z").unwrap();
        let json_str = &json_outputs[0].1;

        // Parse and check that keys are in alphabetical order
        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let keys: Vec<&str> = json["options"]
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();

        assert_eq!(keys, vec!["bool_val", "int_val", "string_val"]);
    }

    #[test]
    fn test_various_value_types() {
        let f = TestFixture::new(&["test"]);
        f.create_file(
            "test",
            "default",
            "base.yaml",
            r#"options:
  string_val: "hello"
  int_val: 42
  float_val: 7.77
  bool_val: true
"#,
        );

        let result = f.load();
        assert!(result.is_ok());

        let grouped = result.unwrap();
        let json_outputs = generate_json(grouped, "2026-01-21T00:00:00Z").unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_outputs[0].1).unwrap();

        assert_eq!(json["options"]["string_val"], "hello");
        assert_eq!(json["options"]["int_val"], 42);
        assert_eq!(json["options"]["float_val"], 7.77);
        assert_eq!(json["options"]["bool_val"], true);
    }

    #[test]
    fn test_files_sorted_alphabetically() {
        let f = TestFixture::new(&["test"]);
        // Create files in non-alphabetical order
        f.create_file(
            "test",
            "default",
            "z_file.yaml",
            &valid_yaml(&[("string_val", "\"v1\"")]),
        );
        f.create_file(
            "test",
            "default",
            "a_file.yaml",
            &valid_yaml(&[("int_val", "42")]),
        );
        f.create_file(
            "test",
            "default",
            "m_file.yaml",
            &valid_yaml(&[("bool_val", "true")]),
        );

        let grouped = f.load().unwrap();
        let files = grouped.get("test").unwrap().get("default").unwrap();

        // Verify files are sorted alphabetically
        for i in 0..files.len() - 1 {
            assert!(files[i].path < files[i + 1].path);
        }
    }

    #[test]
    fn test_unknown_namespace_rejected() {
        let f = TestFixture::new(&[]);
        f.create_file(
            "unknown_ns",
            "default",
            "base.yaml",
            &valid_yaml(&[("string_val", "\"value1\"")]),
        );

        let result = f.load();
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("Unknown namespace"));
                assert!(msg.contains("unknown_ns"));
            }
            _ => panic!("Expected Validation error for unknown namespace"),
        }
    }
}
