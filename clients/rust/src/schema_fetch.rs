//! Fetch schema snapshots from the repositories listed in `repos.json`.
//!
//! This is an explicit tooling API. The options runtime remains file-based and
//! never fetches schemas as part of initialization or value reads.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaFetchError {
    #[error("{0}")]
    Validation(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Git command failed: {0}")]
    Git(String),
}

pub type SchemaFetchResult<T> = std::result::Result<T, SchemaFetchError>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoSchemaConfig {
    pub url: String,
    /// Optional repository pin retained for compatibility with `repos.json`.
    /// Schema snapshots currently follow the repository's default branch.
    #[allow(dead_code)]
    #[serde(default)]
    pub sha: Option<String>,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoSchemaConfigs {
    pub repos: HashMap<String, RepoSchemaConfig>,
}

impl RepoSchemaConfigs {
    pub fn from_file(path: &Path) -> SchemaFetchResult<Self> {
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }
}

/// Fetch every configured repository's schemas into a new directory.
pub fn fetch_schemas(config: &RepoSchemaConfigs, out_dir: &Path) -> SchemaFetchResult<()> {
    if out_dir.exists() {
        return Err(SchemaFetchError::Validation(format!(
            "Output directory already exists: {}",
            out_dir.display()
        )));
    }
    fs::create_dir_all(out_dir)?;

    // Sort repo names for deterministic error reporting.
    let mut repo_names: Vec<_> = config.repos.keys().collect();
    repo_names.sort();

    let results: Vec<_> = thread::scope(|scope| {
        let handles: Vec<_> = repo_names
            .iter()
            .map(|repo_name| {
                let source = &config.repos[*repo_name];
                let name = repo_name.as_str();
                (
                    name,
                    scope.spawn(move || fetch_repo_schemas(name, source, out_dir)),
                )
            })
            .collect();

        handles
            .into_iter()
            .map(|(name, handle)| {
                let result = match handle.join() {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err(format!("{}: thread panicked", name)),
                };
                (name, result)
            })
            .collect::<Vec<_>>()
    });

    let mut errors = Vec::new();
    for (_repo_name, result) in results {
        match result {
            Ok(()) => {}
            Err(error) => errors.push(error),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(SchemaFetchError::Git(errors.join("\n")))
    }
}

/// Read `repos.json` and fetch its configured schemas.
pub fn fetch_schemas_from_file(config_path: &Path, out_dir: &Path) -> SchemaFetchResult<()> {
    let config = RepoSchemaConfigs::from_file(config_path)?;
    fetch_schemas(&config, out_dir)
}

const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(3);

fn fetch_repo_schemas(
    repo_name: &str,
    source: &RepoSchemaConfig,
    out_dir: &Path,
) -> SchemaFetchResult<()> {
    let mut delay = INITIAL_RETRY_DELAY;
    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            thread::sleep(delay);
            delay *= 2;
        }

        match try_fetch_repo_schemas(repo_name, source, out_dir) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.expect("at least one fetch attempt must run"))
}

fn try_fetch_repo_schemas(
    repo_name: &str,
    source: &RepoSchemaConfig,
    out_dir: &Path,
) -> SchemaFetchResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let repo_path = temp_dir.path().join("repo");
    let repo_str = repo_path.to_str().ok_or_else(|| {
        SchemaFetchError::Validation(format!("{}: temp path contains invalid UTF-8", repo_name))
    })?;

    git(
        &[
            "clone",
            "--filter=blob:none",
            "--sparse",
            &source.url,
            repo_str,
        ],
        None,
        repo_name,
    )?;
    git(
        &["sparse-checkout", "set", &source.path],
        Some(&repo_path),
        repo_name,
    )?;

    copy_schemas(&repo_path.join(&source.path), out_dir)
}

fn git(args: &[&str], cwd: Option<&Path>, repo_name: &str) -> SchemaFetchResult<()> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(directory) = cwd {
        command.current_dir(directory);
    }
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(SchemaFetchError::Git(format!(
            "{}: {}",
            repo_name,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn copy_schemas(src: &Path, dest: &Path) -> SchemaFetchResult<()> {
    if !src.exists() {
        return Err(SchemaFetchError::Validation(format!(
            "Schemas path does not exist: {}",
            src.display()
        )));
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let namespace = entry.file_name();
            let src_schema = entry.path().join("schema.json");
            if src_schema.exists() {
                let dest_namespace = dest.join(&namespace);
                fs::create_dir_all(&dest_namespace)?;
                fs::copy(src_schema, dest_namespace.join("schema.json"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_from_file_valid_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(
            &path,
            r#"{
                "repos": {
                    "sentry": {
                        "url": "https://github.com/getsentry/sentry",
                        "sha": "abc123",
                        "path": "schemas/"
                    }
                }
            }"#,
        )
        .unwrap();

        let config = RepoSchemaConfigs::from_file(&path).unwrap();
        assert_eq!(config.repos.len(), 1);
        let sentry = config.repos.get("sentry").unwrap();
        assert_eq!(sentry.url, "https://github.com/getsentry/sentry");
        assert_eq!(sentry.sha, Some("abc123".to_string()));
        assert_eq!(sentry.path, "schemas/");
    }

    #[test]
    fn test_from_file_rejects_unknown_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(
            &path,
            r#"{
                "repos": {
                    "sentry": {
                        "url": "https://github.com/getsentry/sentry",
                        "path": "schemas/",
                        "unknown_field": true
                    }
                }
            }"#,
        )
        .unwrap();

        let error = RepoSchemaConfigs::from_file(&path).unwrap_err();
        assert!(error.to_string().contains("unknown field `unknown_field`"));
    }

    #[test]
    fn test_copy_schemas() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let namespace = src_dir.path().join("relay");
        fs::create_dir_all(&namespace).unwrap();
        fs::write(namespace.join("schema.json"), r#"{"version": "1.0"}"#).unwrap();

        copy_schemas(src_dir.path(), dest_dir.path()).unwrap();

        assert!(dest_dir.path().join("relay/schema.json").exists());
    }

    #[test]
    fn test_fetch_schemas_errors_if_output_exists() {
        let out_dir = TempDir::new().unwrap();
        let config = RepoSchemaConfigs {
            repos: HashMap::new(),
        };

        let error = fetch_schemas(&config, out_dir.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Output directory already exists")
        );
    }
}
