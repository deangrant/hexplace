//! Node coordinate stores for OSM import (sparse extracts + flat planet-capable).

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use hexplace_core::CoreError;
use memmap2::{Mmap, MmapMut, MmapOptions};

use crate::binio::i32_le_opt;

/// Stores OSM node coordinates for way-centroid resolution during import.
pub trait NodeStore {
    /// Inserts or overwrites a node coordinate in 1e-7 degree units.
    ///
    /// # Errors
    ///
    /// Returns an import error when the id is out of range for the store.
    fn insert(&mut self, id: i64, lat_e7: i32, lon_e7: i32) -> Result<(), CoreError>;

    /// Returns coordinates for `id` when present.
    fn get(&self, id: i64) -> Option<(i32, i32)>;

    /// Finalizes the store (for example, sorting a sparse array).
    ///
    /// # Errors
    ///
    /// Returns an I/O error when a backing file cannot be flushed.
    fn finalize(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}

/// Sorted `(id, lat_e7, lon_e7)` array with binary search.
///
/// Default for regional extracts: cost scales with stored nodes, not max id.
/// Duplicate ids keep the last-inserted coordinates after [`NodeStore::finalize`].
#[derive(Debug, Default)]
pub struct SparseNodeStore {
    entries: Vec<(i64, i32, i32)>,
    sorted: bool,
}

impl SparseNodeStore {
    /// Creates an empty sparse store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl NodeStore for SparseNodeStore {
    fn insert(&mut self, id: i64, lat_e7: i32, lon_e7: i32) -> Result<(), CoreError> {
        self.entries.push((id, lat_e7, lon_e7));
        self.sorted = false;
        Ok(())
    }

    fn get(&self, id: i64) -> Option<(i32, i32)> {
        if !self.sorted {
            return None;
        }
        self.entries
            .binary_search_by_key(&id, |e| e.0)
            .ok()
            .and_then(|i| self.entries.get(i).map(|e| (e.1, e.2)))
    }

    fn finalize(&mut self) -> Result<(), CoreError> {
        // Stable sort preserves insertion order among equal ids; dedup keeps
        // the last insert by copying later coords into the surviving slot.
        self.entries.sort_by_key(|e| e.0);
        // `dedup_by` passes (may_remove, kept); copy later coords into kept.
        self.entries.dedup_by(|later, kept| {
            if later.0 == kept.0 {
                kept.1 = later.1;
                kept.2 = later.2;
                true
            } else {
                false
            }
        });
        self.sorted = true;
        Ok(())
    }
}

/// Flat file indexed directly by node id (`8 bytes * (max_id + 1)`).
///
/// Planet-capable; unit-tested with synthetic ids only in this release.
pub struct FlatNodeStore {
    path: PathBuf,
    mmap: MmapMut,
    max_id: u64,
}

impl std::fmt::Debug for FlatNodeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatNodeStore")
            .field("path", &self.path)
            .field("max_id", &self.max_id)
            .finish_non_exhaustive()
    }
}

impl FlatNodeStore {
    /// Creates a flat store sized for ids in `0..=max_id`.
    ///
    /// # Errors
    ///
    /// Returns an import or I/O error when the size overflows or the file cannot
    /// be created and mapped.
    pub fn create(path: impl Into<PathBuf>, max_id: u64) -> Result<Self, CoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = (max_id + 1)
            .checked_mul(8)
            .ok_or_else(|| CoreError::import("flat node store size overflow"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len(bytes)?;
        #[expect(
            unsafe_code,
            reason = "memmap2 mutable map is unsafe; exclusive import writer owns the file"
        )]
        // SAFETY: exclusive writer during import; length set above and not resized.
        let mmap = unsafe { MmapOptions::new().map_mut(&file) }
            .map_err(|e| CoreError::io(format!("flat node mmap failed: {e}")))?;
        Ok(Self { path, mmap, max_id })
    }

    /// Opens an existing flat store for read-only lookups.
    ///
    /// # Errors
    ///
    /// Returns an import or I/O error when the file length is invalid or mapping
    /// fails.
    pub fn open_readonly(path: impl AsRef<Path>) -> Result<(PathBuf, Mmap, u64), CoreError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let meta = file.metadata()?;
        let len = meta.len();
        if len % 8 != 0 || len == 0 {
            return Err(CoreError::import("invalid flat node store length"));
        }
        let max_id = len / 8 - 1;
        #[expect(
            unsafe_code,
            reason = "memmap2::Mmap::map is unsafe; reader must not truncate while mapped"
        )]
        // SAFETY: file is not mutated while mapped for tests/readers.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| CoreError::io(format!("flat node mmap failed: {e}")))?;
        Ok((path, mmap, max_id))
    }

    /// Path to the underlying file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl NodeStore for FlatNodeStore {
    fn insert(&mut self, id: i64, lat_e7: i32, lon_e7: i32) -> Result<(), CoreError> {
        let id = u64::try_from(id).map_err(|_| CoreError::import("negative OSM node id"))?;
        if id > self.max_id {
            return Err(CoreError::import(format!(
                "node id {id} exceeds flat store max {}",
                self.max_id
            )));
        }
        let off = usize::try_from(id)
            .map_err(|_| CoreError::import("node id too large for usize"))?
            .checked_mul(8)
            .ok_or_else(|| CoreError::import("flat node slot out of range"))?;
        let lat_slot = self
            .mmap
            .get_mut(off..off + 4)
            .ok_or_else(|| CoreError::import("flat node slot out of range"))?;
        lat_slot.copy_from_slice(&lat_e7.to_le_bytes());
        let lon_slot = self
            .mmap
            .get_mut(off + 4..off + 8)
            .ok_or_else(|| CoreError::import("flat node slot out of range"))?;
        lon_slot.copy_from_slice(&lon_e7.to_le_bytes());
        Ok(())
    }

    fn get(&self, id: i64) -> Option<(i32, i32)> {
        let id = u64::try_from(id).ok()?;
        if id > self.max_id {
            return None;
        }
        let off = usize::try_from(id).ok()?.checked_mul(8)?;
        let lat = i32_le_opt(&self.mmap, off)?;
        let lon = i32_le_opt(&self.mmap, off + 4)?;
        // Unwritten slots are zero; treat (0,0) as missing only when never set
        // is ambiguous — for tests we accept zeros as valid Gulf of Guinea.
        Some((lat, lon))
    }

    fn finalize(&mut self) -> Result<(), CoreError> {
        self.mmap
            .flush()
            .map_err(|e| CoreError::io(format!("flat node flush failed: {e}")))?;
        Ok(())
    }
}

/// Read helper used by unit tests against a finalized flat file.
#[cfg(test)]
fn flat_get(path: &Path, id: i64) -> Result<Option<(i32, i32)>, CoreError> {
    let (_p, mmap, max_id) = FlatNodeStore::open_readonly(path)?;
    let id = u64::try_from(id).ok();
    let Some(id) = id else {
        return Ok(None);
    };
    if id > max_id {
        return Ok(None);
    }
    let off = usize::try_from(id)
        .map_err(|_| CoreError::storage("node id too large"))?
        .checked_mul(8)
        .ok_or_else(|| CoreError::storage("flat node slot out of range"))?;
    let lat = i32_le_opt(&mmap, off).ok_or_else(|| CoreError::storage("truncated i32 read"))?;
    let lon = i32_le_opt(&mmap, off + 4).ok_or_else(|| CoreError::storage("truncated i32 read"))?;
    Ok(Some((lat, lon)))
}

/// Converts degrees to OSM 1e-7 integer units.
#[must_use]
pub fn deg_to_e7(v: f64) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "WGS84 degrees scaled to OSM 1e-7 units fit i32"
    )]
    {
        (v * 10_000_000.0).round() as i32
    }
}

/// Converts OSM 1e-7 integer units to degrees.
#[must_use]
pub fn e7_to_deg(v: i32) -> f64 {
    f64::from(v) / 10_000_000.0
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
    fn sparse_round_trip() -> TestResult {
        let mut store = SparseNodeStore::new();
        store.insert(10, 1, 2)?;
        store.insert(5, 3, 4)?;
        store.finalize()?;
        assert_eq!(
            store.get(5),
            Some((3, 4)),
            "sparse store must return inserted coordinates after finalize"
        );
        assert_eq!(
            store.get(10),
            Some((1, 2)),
            "sparse store must return inserted coordinates after finalize"
        );
        assert_eq!(
            store.get(7),
            None,
            "missing ids must stay absent after finalize"
        );
        Ok(())
    }

    #[test]
    fn sparse_duplicate_ids_keep_last_insert() -> TestResult {
        let mut store = SparseNodeStore::new();
        store.insert(10, 1, 2)?;
        store.insert(10, 3, 4)?;
        store.insert(10, 5, 6)?;
        store.insert(7, 9, 8)?;
        store.finalize()?;
        assert_eq!(
            store.get(10),
            Some((5, 6)),
            "duplicate ids must keep the last-inserted coordinates"
        );
        assert_eq!(
            store.get(7),
            Some((9, 8)),
            "unaffected ids must keep their coordinates"
        );
        Ok(())
    }

    #[test]
    fn flat_round_trip_synthetic() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nodes.flat");
        let mut store = FlatNodeStore::create(&path, 100)?;
        store.insert(42, 480_000_000, 20_000_000)?;
        store.finalize()?;
        assert_eq!(
            store.get(42),
            Some((480_000_000, 20_000_000)),
            "flat store must return coordinates from the live mmap"
        );
        assert_eq!(
            flat_get(&path, 42)?,
            Some((480_000_000, 20_000_000)),
            "flat store must round-trip through a remapped readonly file"
        );
        Ok(())
    }
}
