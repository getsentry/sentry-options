use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{AppError, Result};

/// Fetch multiple SHAs from origin
pub fn fetch_shas(shas: &[&str]) -> Result<()> {
    for sha in shas {
        let output = Command::new("git")
            .args(["fetch", "origin", sha])
            .output()?;

        if !output.status.success() {
            return Err(AppError::Git(format!(
                "Failed to fetch {}: {}",
                sha,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    Ok(())
}

/// Extract schemas directory from the current git repository at a specific SHA
/// SHA must already be fetched (call fetch_shas to ensure this)
pub fn extract_schemas_at_sha(sha: &str, schemas_path: &str, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)?;

    // Use `git ls-tree` to check path existence in a single command.
    // Unlike cat-file, it exits non-zero for an invalid/unreachable SHA and
    // exits 0 with empty stdout when the SHA is valid but the path isn't in
    // the tree.
    let ls_tree = Command::new("git")
        .args(["ls-tree", sha, schemas_path])
        .output()?;

    if !ls_tree.status.success() {
        return Err(AppError::Git(format!(
            "git ls-tree failed for {} at {}: {}",
            schemas_path,
            sha,
            String::from_utf8_lossy(&ls_tree.stderr).trim()
        )));
    }

    if ls_tree.stdout.is_empty() {
        // The schemas directory didn't exist at this SHA (e.g. the repo is
        // onboarding sentry-options for the first time). Create the expected
        // subdirectory so callers see an empty schema set.
        fs::create_dir_all(out_dir.join(schemas_path))?;
        return Ok(());
    }

    // git archive will output a tarball of the specified directory
    let archive_output = Command::new("git")
        .args(["archive", sha, schemas_path])
        .output()?;

    if !archive_output.status.success() {
        return Err(AppError::Git(format!(
            "Failed to archive {} at {}: {}",
            schemas_path,
            sha,
            String::from_utf8_lossy(&archive_output.stderr).trim()
        )));
    }

    let mut tar_child = Command::new("tar")
        .args(["-xf", "-", "-C"])
        .arg(out_dir)
        .stdin(Stdio::piped())
        .spawn()?;

    // avoids writing to a temp file between the git archive and tar -xf
    if let Some(mut stdin) = tar_child.stdin.take() {
        stdin.write_all(&archive_output.stdout)?;
    }

    let status = tar_child.wait()?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "Failed to extract tar archive for {} at {}",
            schemas_path, sha
        )));
    }

    Ok(())
}
