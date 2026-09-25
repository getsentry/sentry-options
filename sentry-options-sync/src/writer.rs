//! Kubelet-style atomic projection of ConfigMap data into a directory.
//!
//! Layout matches a kubelet ConfigMap volume, so readers see the same paths
//! whether the directory is a ConfigMap mount or this sidecar's `emptyDir`:
//!
//! ```text
//! {dir}/..2026_09_25_12_00_00.123456789/values.json   (current payload)
//! {dir}/..data -> ..2026_09_25_12_00_00.123456789      (swapped atomically)
//! {dir}/values.json -> ..data/values.json              (stable user path)
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

const DATA_DIR: &str = "..data";
const DATA_DIR_TMP: &str = "..data_tmp";

/// Replace the payload in `dir` with `data`. An empty map clears the
/// directory, matching an optional ConfigMap volume whose ConfigMap is absent.
pub fn write_atomic(dir: &Path, data: &BTreeMap<String, String>) -> Result<()> {
    let previous = fs::read_link(dir.join(DATA_DIR)).ok();

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let payload_name = format!("..{nanos}");
    let payload = dir.join(&payload_name);
    fs::create_dir(&payload).with_context(|| format!("creating {}", payload.display()))?;
    fs::set_permissions(&payload, fs::Permissions::from_mode(0o755))?;
    for (key, value) in data {
        let path = payload.join(key);
        let mut file = fs::File::create(&path)?;
        file.write_all(value.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    }

    let tmp = dir.join(DATA_DIR_TMP);
    let _ = fs::remove_file(&tmp);
    symlink(&payload_name, &tmp)?;
    fs::rename(&tmp, dir.join(DATA_DIR)).context("swapping ..data")?;

    // User-visible links point through ..data, so they only need to track the
    // key set; their targets change with the swap above.
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("..") && !data.contains_key(&name) {
            fs::remove_file(entry.path())?;
        }
    }
    for key in data.keys() {
        let link = dir.join(key);
        if fs::symlink_metadata(&link).is_err() {
            symlink(Path::new(DATA_DIR).join(key), &link)?;
        }
    }

    if let Some(previous) = previous {
        fs::remove_dir_all(dir.join(previous))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn payload_dirs(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .filter(|e| {
                let name = e.as_ref().unwrap().file_name();
                let name = name.to_string_lossy();
                name.starts_with("..") && name != DATA_DIR
            })
            .count()
    }

    #[test]
    fn writes_and_replaces_payload() {
        let dir = tempfile::tempdir().unwrap();
        write_atomic(dir.path(), &data(&[("values.json", "1")])).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("values.json")).unwrap(),
            "1"
        );

        write_atomic(dir.path(), &data(&[("values.json", "2")])).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("values.json")).unwrap(),
            "2"
        );
        assert_eq!(payload_dirs(dir.path()), 1);
    }

    #[test]
    fn removes_keys_no_longer_present() {
        let dir = tempfile::tempdir().unwrap();
        write_atomic(dir.path(), &data(&[("a", "1"), ("b", "2")])).unwrap();
        write_atomic(dir.path(), &data(&[("a", "3")])).unwrap();
        assert!(fs::symlink_metadata(dir.path().join("b")).is_err());

        write_atomic(dir.path(), &BTreeMap::new()).unwrap();
        assert!(fs::symlink_metadata(dir.path().join("a")).is_err());
        assert_eq!(payload_dirs(dir.path()), 1);
    }
}
