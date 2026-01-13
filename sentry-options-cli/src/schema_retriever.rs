use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;

use crate::repo_schema_config::{RepoSchemaConfig, RepoSchemaConfigs};
use crate::{AppError, Result};

pub fn fetch_all_schemas(config: &RepoSchemaConfigs, out_dir: &Path, quiet: bool) -> Result<()> {
    if out_dir.exists() {
        return Err(AppError::Validation(format!(
            "Output directory already exists: {}",
            out_dir.display()
        )));
    }
    fs::create_dir_all(out_dir)?;

    // Sort repo names for deterministic ordering
    let mut repo_names: Vec<_> = config.repos.keys().collect();
    repo_names.sort();

    if !quiet {
        println!("Fetching schemas...");
    }

    // Fetch all repos in parallel, collecting results
    let results: Vec<_> = thread::scope(|s| {
        let handles: Vec<_> = repo_names
            .iter()
            .map(|repo_name| {
                let source = &config.repos[*repo_name];
                let name = repo_name.as_str();
                (
                    name,
                    s.spawn(move || fetch_repo_schemas(name, source, out_dir)),
                )
            })
            .collect();

        handles
            .into_iter()
            .map(|(name, h)| {
                let result: std::result::Result<(), String> = match h.join() {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err(format!("{}: thread panicked", name)),
                };
                (name, result)
            })
            .collect::<Vec<_>>()
    });

    // Report results in deterministic order
    let mut errors: Vec<String> = Vec::new();
    for (repo_name, result) in results {
        match result {
            Ok(()) => {
                if !quiet {
                    println!("  Fetched {}", repo_name);
                }
            }
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        return Err(AppError::Git(errors.join("\n")));
    }

    Ok(())
}

fn fetch_repo_schemas(repo_name: &str, source: &RepoSchemaConfig, out_dir: &Path) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let repo_path = temp_dir.path().join("repo");
    let repo_str = repo_path.to_str().ok_or_else(|| {
        AppError::Validation(format!("{}: temp path contains invalid UTF-8", repo_name))
    })?;

    // Partial clone to reduce download size, and only fetch the schemas we need
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
    git(&["checkout", &source.sha], Some(&repo_path), repo_name)?;

    copy_schemas(&repo_path.join(&source.path), out_dir)?;
    Ok(())
}

fn git(args: &[&str], cwd: Option<&Path>, repo_name: &str) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(AppError::Git(format!(
            "{}: {}",
            repo_name,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn copy_schemas(src: &Path, dest: &Path) -> Result<()> {
    if !src.exists() {
        return Err(AppError::Validation(format!(
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
                let dest_ns = dest.join(&namespace);
                fs::create_dir_all(&dest_ns)?;
                fs::copy(&src_schema, dest_ns.join("schema.json"))?;
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
    fn test_copy_schemas_valid_structure() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        let ns1 = src_dir.path().join("relay");
        let ns2 = src_dir.path().join("billing");
        fs::create_dir_all(&ns1).unwrap();
        fs::create_dir_all(&ns2).unwrap();
        fs::write(ns1.join("schema.json"), r#"{"version": "1.0"}"#).unwrap();
        fs::write(ns2.join("schema.json"), r#"{"version": "1.0"}"#).unwrap();

        let dest = dest_dir.path().join("sentry");
        copy_schemas(src_dir.path(), &dest).unwrap();

        assert!(dest.join("relay/schema.json").exists());
        assert!(dest.join("billing/schema.json").exists());
    }

    #[test]
    fn test_copy_schemas_missing_src() {
        let dest_dir = TempDir::new().unwrap();
        let err = copy_schemas(Path::new("/nonexistent/path"), dest_dir.path()).unwrap_err();
        assert!(err.to_string().contains("Schemas path does not exist"));
    }

    #[test]
    fn test_copy_schemas_skips_non_directories() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        fs::write(src_dir.path().join("README.md"), "# Schemas").unwrap();
        let ns = src_dir.path().join("relay");
        fs::create_dir_all(&ns).unwrap();
        fs::write(ns.join("schema.json"), r#"{"version": "1.0"}"#).unwrap();

        let dest = dest_dir.path().join("sentry");
        copy_schemas(src_dir.path(), &dest).unwrap();

        assert!(dest.join("relay/schema.json").exists());
        assert!(!dest.join("README.md").exists());
    }

    #[test]
    fn test_copy_schemas_skips_dirs_without_schema() {
        let src_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        fs::create_dir_all(src_dir.path().join("empty")).unwrap();
        let ns = src_dir.path().join("relay");
        fs::create_dir_all(&ns).unwrap();
        fs::write(ns.join("schema.json"), r#"{"version": "1.0"}"#).unwrap();

        let dest = dest_dir.path().join("sentry");
        copy_schemas(src_dir.path(), &dest).unwrap();

        assert!(dest.join("relay/schema.json").exists());
        assert!(!dest.join("empty").exists());
    }

    #[test]
    fn test_fetch_all_schemas_errors_if_output_exists() {
        use crate::repo_schema_config::RepoSchemaConfigs;

        let out_dir = TempDir::new().unwrap();
        let existing_dir = out_dir.path().join("schemas");
        fs::create_dir_all(&existing_dir).unwrap();

        let config: RepoSchemaConfigs = serde_json::from_str(r#"{"repos": {}}"#).unwrap();
        let err = fetch_all_schemas(&config, &existing_dir, true).unwrap_err();
        assert!(err.to_string().contains("Output directory already exists"));
    }
}
