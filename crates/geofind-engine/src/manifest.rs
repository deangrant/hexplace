//! Data-directory layout and manifest metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use geofind_core::CoreError;
use serde::{Deserialize, Serialize};

/// On-disk schema version; bump when the binary format changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Paths inside a Geofind data directory.
#[derive(Debug, Clone)]
pub struct DataPaths {
    /// Root data directory.
    pub root: PathBuf,
}

impl DataPaths {
    /// Creates path helpers for `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Manifest file path.
    pub fn manifest(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    /// Place record file path.
    pub fn places(&self) -> PathBuf {
        self.root.join("places.bin")
    }

    /// Tantivy text index directory.
    pub fn text_dir(&self) -> PathBuf {
        self.root.join("text")
    }

    /// H3 spatial postings file.
    pub fn spatial(&self) -> PathBuf {
        self.root.join("spatial").join("h3.bin")
    }

    /// Ensures the directory tree exists for a fresh import.
    pub fn ensure_layout(&self) -> Result<(), CoreError> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(self.text_dir())?;
        fs::create_dir_all(self.root.join("spatial"))?;
        Ok(())
    }
}

/// Metadata written after a successful import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    /// Binary schema version.
    pub schema_version: u32,
    /// Source PBF path as provided at import time.
    pub source_path: String,
    /// Hex-encoded FNV-1a hash of the source file bytes (sampled for large files).
    pub source_hash: String,
    /// Number of indexed places.
    pub place_count: u64,
    /// H3 resolution used for the spatial index.
    pub h3_resolution: u8,
    /// Unix timestamp (seconds) when the index was built.
    pub built_at_unix: u64,
}

impl Manifest {
    /// Creates a new manifest for a completed import.
    pub fn new(
        source_path: impl Into<String>,
        source_hash: impl Into<String>,
        place_count: u64,
        h3_resolution: u8,
    ) -> Self {
        let built_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            schema_version: SCHEMA_VERSION,
            source_path: source_path.into(),
            source_hash: source_hash.into(),
            place_count,
            h3_resolution,
            built_at_unix,
        }
    }

    /// Loads a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let bytes = fs::read(path)
            .map_err(|e| CoreError::io(format!("failed to read {}: {e}", path.display())))?;
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::storage(format!("invalid manifest: {e}")))?;
        if manifest.schema_version != SCHEMA_VERSION {
            return Err(CoreError::storage(format!(
                "unsupported schema version {} (expected {SCHEMA_VERSION})",
                manifest.schema_version
            )));
        }
        Ok(manifest)
    }

    /// Writes the manifest as pretty JSON.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| CoreError::storage(format!("manifest encode failed: {e}")))?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// Computes a fast content fingerprint for import provenance.
pub fn hash_file(path: &Path) -> Result<String, CoreError> {
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; 64 * 1024];
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut total = 0usize;
    // Hash up to the first 8 MiB so huge planet files stay practical.
    const LIMIT: usize = 8 * 1024 * 1024;
    loop {
        if total >= LIMIT {
            break;
        }
        let want = (LIMIT - total).min(buf.len());
        let n = file.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        total += n;
    }
    Ok(format!("{hash:016x}"))
}
