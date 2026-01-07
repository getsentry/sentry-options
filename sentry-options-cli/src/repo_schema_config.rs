use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::Result;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoSchemaConfig {
    pub url: String,
    pub sha: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoSchemaConfigs {
    pub repos: HashMap<String, RepoSchemaConfig>,
}

impl RepoSchemaConfigs {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: RepoSchemaConfigs = serde_json::from_str(&content)?;
        Ok(config)
    }
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
                    },
                    "getsentry": {
                        "url": "https://github.com/getsentry/getsentry",
                        "sha": "def456",
                        "path": "schemas/"
                    }
                }
            }"#,
        )
        .unwrap();

        let config = RepoSchemaConfigs::from_file(&path).unwrap();
        assert_eq!(config.repos.len(), 2);

        let sentry = config.repos.get("sentry").unwrap();
        assert_eq!(sentry.url, "https://github.com/getsentry/sentry");
        assert_eq!(sentry.sha, "abc123");
        assert_eq!(sentry.path, "schemas/");

        let getsentry = config.repos.get("getsentry").unwrap();
        assert_eq!(getsentry.url, "https://github.com/getsentry/getsentry");
        assert_eq!(getsentry.sha, "def456");
        assert_eq!(getsentry.path, "schemas/");
    }

    #[test]
    fn test_from_file_missing_file() {
        let err = RepoSchemaConfigs::from_file(Path::new("/nonexistent/repos.json")).unwrap_err();
        assert!(err.to_string().contains("No such file or directory"));
    }

    #[test]
    fn test_from_file_invalid_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(&path, "{ invalid json }").unwrap();

        let err = RepoSchemaConfigs::from_file(&path).unwrap_err();
        assert!(err.to_string().contains("key must be a string"));
    }

    #[test]
    fn test_from_file_unknown_fields_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(
            &path,
            r#"{
                "repos": {
                    "sentry": {
                        "url": "https://github.com/getsentry/sentry",
                        "sha": "abc123",
                        "path": "schemas/",
                        "unknown_field": "should fail"
                    }
                }
            }"#,
        )
        .unwrap();

        let err = RepoSchemaConfigs::from_file(&path).unwrap_err();
        assert!(err.to_string().contains("unknown field `unknown_field`"));
    }

    #[test]
    fn test_from_file_empty_repos() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(&path, r#"{"repos": {}}"#).unwrap();

        let config = RepoSchemaConfigs::from_file(&path).unwrap();
        assert!(config.repos.is_empty());
    }

    #[test]
    fn test_from_file_missing_required_field() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("repos.json");
        fs::write(
            &path,
            r#"{
                "repos": {
                    "sentry": {
                        "url": "https://github.com/getsentry/sentry",
                        "sha": "abc123"
                    }
                }
            }"#,
        )
        .unwrap();

        let err = RepoSchemaConfigs::from_file(&path).unwrap_err();
        assert!(err.to_string().contains("missing field `path`"));
    }
}
