//! Atomic publish of a staged data directory into the live path.

use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs4::{FileExt, TryLockError};
use hexplace_core::CoreError;

/// Lock file held shared by serve and exclusive during publish.
pub const LOCK_FILE_NAME: &str = ".lock";

/// Creates a unique staging directory beside `target`.
///
/// # Errors
///
/// Returns an import error when the staging directory cannot be created.
pub fn create_staging_dir(target: &Path) -> Result<PathBuf, CoreError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("data");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let staging = parent.join(format!(".{name}.staging-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&staging).map_err(|e| {
        CoreError::import(format!(
            "failed to create staging {}: {e}",
            staging.display()
        ))
    })?;
    Ok(staging)
}

/// Removes leftover staging/obsolete siblings from prior crashed imports.
///
/// # Errors
///
/// Returns an I/O error when parent listing or legacy-file removal fails.
pub fn cleanup_orphans(target: &Path) -> Result<(), CoreError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("data");
    let staging_prefix = format!(".{name}.staging-");
    let obsolete_prefix = format!(".{name}.obsolete-");
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(CoreError::io(format!(
                "failed to list {}: {e}",
                parent.display()
            )));
        }
    };
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(format!("readdir failed: {e}")))?;
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        if name_str.starts_with(&staging_prefix) || name_str.starts_with(&obsolete_prefix) {
            let path = entry.path();
            let _ = fs::remove_dir_all(&path);
        }
    }
    // Pre-columnar leftover beside places/; ignore missing file.
    let legacy_places = target.join("places.bin");
    match fs::remove_file(&legacy_places) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => {
            return Err(CoreError::io(format!(
                "failed to remove {}: {e}",
                legacy_places.display()
            )));
        }
    }
    Ok(())
}

/// Opens (or creates) the data-dir lock and acquires a shared flock.
///
/// # Errors
///
/// Returns an I/O error when the lock file cannot be opened or locked.
pub fn acquire_shared_lock(data_dir: &Path) -> Result<File, CoreError> {
    let path = data_dir.join(LOCK_FILE_NAME);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    FileExt::lock_shared(&file)
        .map_err(|e| CoreError::io(format!("failed to lock {}: {e}", path.display())))?;
    Ok(file)
}

/// Publishes `staging` over `target` via directory renames.
///
/// If `target` already exists, requires an exclusive lock on its `.lock` file
/// so a running serve process cannot be swapped out from under mmap/Tantivy.
///
/// # Errors
///
/// Returns an import error when the exclusive lock is held, a rename fails, or
/// parent directories cannot be created. Returns an I/O error for lock-file open
/// failures other than contention.
pub fn publish_data_dir(staging: &Path, target: &Path) -> Result<(), CoreError> {
    let _exclusive = if target.exists() {
        Some(acquire_exclusive_lock(target)?)
    } else {
        None
    };

    let obsolete = if target.exists() {
        let path = obsolete_path(target);
        fs::rename(target, &path).map_err(|e| {
            CoreError::import(format!("failed to move {} aside: {e}", target.display()))
        })?;
        Some(path)
    } else {
        None
    };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CoreError::import(format!("failed to create {}: {e}", parent.display()))
        })?;
    }

    match fs::rename(staging, target) {
        Ok(()) => {}
        Err(e) => {
            if let Some(ref obsolete) = obsolete {
                let _ = fs::rename(obsolete, target);
            }
            return Err(CoreError::import(format!(
                "failed to publish staging to {}: {e}",
                target.display()
            )));
        }
    }

    if let Some(obsolete) = obsolete {
        let _ = fs::remove_dir_all(obsolete);
    }
    Ok(())
}

/// Best-effort removal of a staging directory after a failed import.
pub fn discard_staging(staging: &Path) {
    let _ = fs::remove_dir_all(staging);
}

/// Opens the data-dir lock and takes an exclusive non-blocking flock.
///
/// # Errors
///
/// Returns an import error when another process holds the lock (`WouldBlock`),
/// or an I/O error when the lock file cannot be opened or locked otherwise.
fn acquire_exclusive_lock(data_dir: &Path) -> Result<File, CoreError> {
    let path = data_dir.join(LOCK_FILE_NAME);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    match FileExt::try_lock(&file) {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(CoreError::import(
            "data directory in use; stop serve before re-importing",
        )),
        Err(e) => Err(CoreError::io(format!(
            "failed to lock {}: {e}",
            path.display()
        ))),
    }
}

fn obsolete_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("data");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    parent.join(format!(".{name}.obsolete-{}-{stamp}", std::process::id()))
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn cleanup_orphans_removes_legacy_places_bin() -> TestResult {
        let parent = tempfile::tempdir()?;
        let data = parent.path().join("data");
        fs::create_dir_all(&data)?;
        let legacy = data.join("places.bin");
        fs::write(&legacy, b"obsolete")?;
        cleanup_orphans(&data)?;
        assert!(
            !legacy.exists(),
            "legacy places.bin beside the data dir must be removed"
        );
        Ok(())
    }
}
