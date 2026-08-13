//! Data-directory layout and manifest metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hexplace_core::CoreError;
use serde::{Deserialize, Serialize};

use crate::spatial::SpatialPaths;
use crate::spatial::{H3_RESOLUTION_COARSE, H3_RESOLUTION_FINE};
use crate::store::PlacePaths;

/// On-disk schema version; bump when the binary format changes.
pub const SCHEMA_VERSION: u32 = 2;

/// Store format marker for columnar places.
pub const STORE_FORMAT_COLUMNAR: &str = "columnar_v2";
/// Spatial format marker for CSR H3 indexes.
pub const SPATIAL_FORMAT_CSR: &str = "csr_h3_v2";

/// Paths inside a Hexplace data directory.
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

    /// Columnar place store paths.
    pub fn places(&self) -> PlacePaths {
        PlacePaths::new(self.root.join("places"))
    }

    /// Tantivy text index directory.
    pub fn text_dir(&self) -> PathBuf {
        self.root.join("text")
    }

    /// CSR spatial index paths.
    pub fn spatial(&self) -> SpatialPaths {
        SpatialPaths::new(self.root.join("spatial"))
    }

    /// Ensures the directory tree exists for a fresh import.
    pub fn ensure_layout(&self) -> Result<(), CoreError> {
        fs::create_dir_all(&self.root)?;
        self.places().ensure()?;
        fs::create_dir_all(self.text_dir())?;
        self.spatial().ensure()?;
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
    /// Hex-encoded SHA-256 of the full source file bytes.
    pub source_hash: String,
    /// Number of indexed places.
    pub place_count: u64,
    /// Fine H3 resolution used for reverse lookup.
    pub h3_resolution_fine: u8,
    /// Coarse H3 resolution used for empty-cell fallback.
    pub h3_resolution_coarse: u8,
    /// Place store format marker.
    pub store_format: String,
    /// Spatial index format marker.
    pub spatial_format: String,
    /// Unix timestamp (seconds) when the index was built.
    pub built_at_unix: u64,
}

impl Manifest {
    /// Creates a new manifest for a completed import.
    pub fn new(
        source_path: impl Into<String>,
        source_hash: impl Into<String>,
        place_count: u64,
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
            h3_resolution_fine: H3_RESOLUTION_FINE,
            h3_resolution_coarse: H3_RESOLUTION_COARSE,
            store_format: STORE_FORMAT_COLUMNAR.to_owned(),
            spatial_format: SPATIAL_FORMAT_CSR.to_owned(),
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
        if manifest.store_format != STORE_FORMAT_COLUMNAR {
            return Err(CoreError::storage(format!(
                "unsupported store format {}",
                manifest.store_format
            )));
        }
        if manifest.spatial_format != SPATIAL_FORMAT_CSR {
            return Err(CoreError::storage(format!(
                "unsupported spatial format {}",
                manifest.spatial_format
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

/// Computes a SHA-256 digest of the full file for import provenance.
pub fn hash_file(path: &Path) -> Result<String, CoreError> {
    use std::io::Read;

    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or(&[]));
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_file_matches_known_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.bin");
        fs::write(&path, b"hello").unwrap();
        let digest = hash_file(&path).unwrap();
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn hash_file_covers_bytes_past_eight_mib() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = vec![0u8; 8 * 1024 * 1024];
        let a_path = dir.path().join("a.bin");
        let b_path = dir.path().join("b.bin");
        {
            let mut a = fs::File::create(&a_path).unwrap();
            a.write_all(&prefix).unwrap();
            a.write_all(b"A").unwrap();
        }
        {
            let mut b = fs::File::create(&b_path).unwrap();
            b.write_all(&prefix).unwrap();
            b.write_all(b"B").unwrap();
        }
        let ha = hash_file(&a_path).unwrap();
        let hb = hash_file(&b_path).unwrap();
        assert_ne!(ha, hb);
        assert_eq!(ha.len(), 64);
        assert_eq!(hb.len(), 64);
    }

    #[test]
    fn load_rejects_wrong_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.schema_version = SCHEMA_VERSION + 1;
        manifest.save(&path).unwrap();
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.to_string().contains("schema version"));
    }

    #[test]
    fn load_rejects_wrong_store_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.store_format = "legacy".into();
        manifest.save(&path).unwrap();
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.to_string().contains("store format"));
    }

    #[test]
    fn load_rejects_wrong_spatial_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.spatial_format = "legacy".into();
        manifest.save(&path).unwrap();
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.to_string().contains("spatial format"));
    }
}
