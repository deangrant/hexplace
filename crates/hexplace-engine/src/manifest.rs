//! Data-directory layout and manifest metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hexplace_core::CoreError;
use serde::{Deserialize, Serialize};

use crate::spatial::{SpatialPaths, H3_RESOLUTION_COARSE, H3_RESOLUTION_FINE};
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
    #[must_use]
    pub fn manifest(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    /// Columnar place store paths.
    #[must_use]
    pub fn places(&self) -> PlacePaths {
        PlacePaths::new(self.root.join("places"))
    }

    /// Tantivy text index directory.
    #[must_use]
    pub fn text_dir(&self) -> PathBuf {
        self.root.join("text")
    }

    /// CSR spatial index paths.
    #[must_use]
    pub fn spatial(&self) -> SpatialPaths {
        SpatialPaths::new(self.root.join("spatial"))
    }

    /// Ensures the directory tree exists for a fresh import.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when any required directory cannot be created.
    pub fn ensure_layout(&self) -> Result<(), CoreError> {
        fs::create_dir_all(&self.root)?;
        self.places().ensure()?;
        fs::create_dir_all(self.text_dir())?;
        self.spatial().ensure()?;
        Ok(())
    }
}

/// Metadata written after a successful import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        let built_at_unix =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
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
    ///
    /// # Errors
    ///
    /// Returns an I/O or storage error when the file is missing, invalid JSON,
    /// or has an unsupported schema/format marker.
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
    ///
    /// # Errors
    ///
    /// Returns a storage or I/O error when encoding or writing fails.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| CoreError::storage(format!("manifest encode failed: {e}")))?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// Computes a SHA-256 digest of the full file for import provenance.
///
/// # Errors
///
/// Returns an I/O error when the file cannot be read.
pub fn hash_file(path: &Path) -> Result<String, CoreError> {
    use std::fmt::Write as _;
    use std::io::Read;

    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or(&[]));
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    Ok(out)
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]
mod tests {
    use std::io::Write;

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn hash_file_matches_known_sha256() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sample.bin");
        fs::write(&path, b"hello")?;
        let digest = hash_file(&path)?;
        assert_eq!(
            digest, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "SHA-256 of the literal bytes `hello` must match the known digest"
        );
        Ok(())
    }

    #[test]
    fn hash_file_covers_bytes_past_eight_mib() -> TestResult {
        let dir = tempfile::tempdir()?;
        let prefix = vec![0u8; 8 * 1024 * 1024];
        let a_path = dir.path().join("a.bin");
        let b_path = dir.path().join("b.bin");
        {
            let mut a = fs::File::create(&a_path)?;
            a.write_all(&prefix)?;
            a.write_all(b"A")?;
        }
        {
            let mut b = fs::File::create(&b_path)?;
            b.write_all(&prefix)?;
            b.write_all(b"B")?;
        }
        let ha = hash_file(&a_path)?;
        let hb = hash_file(&b_path)?;
        assert_ne!(
            ha, hb,
            "files that differ past 8 MiB must not share a digest"
        );
        assert_eq!(
            ha.len(),
            64,
            "SHA-256 hex encoding must always be 64 lowercase digits"
        );
        assert_eq!(
            hb.len(),
            64,
            "SHA-256 hex encoding must always be 64 lowercase digits"
        );
        Ok(())
    }

    #[test]
    fn load_rejects_wrong_schema_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.schema_version = SCHEMA_VERSION + 1;
        manifest.save(&path)?;
        let Err(err) = Manifest::load(&path) else {
            return Err("wrong schema version must fail load".into());
        };
        assert!(
            err.to_string().contains("schema version"),
            "schema mismatch error must mention schema version"
        );
        Ok(())
    }

    #[test]
    fn load_rejects_wrong_store_format() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.store_format = "legacy".into();
        manifest.save(&path)?;
        let Err(err) = Manifest::load(&path) else {
            return Err("wrong store format must fail load".into());
        };
        assert!(
            err.to_string().contains("store format"),
            "store format mismatch error must mention store format"
        );
        Ok(())
    }

    #[test]
    fn load_rejects_wrong_spatial_format() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("manifest.json");
        let mut manifest = Manifest::new("memory://x", "abc", 1);
        manifest.spatial_format = "legacy".into();
        manifest.save(&path)?;
        let Err(err) = Manifest::load(&path) else {
            return Err("wrong spatial format must fail load".into());
        };
        assert!(
            err.to_string().contains("spatial format"),
            "spatial format mismatch error must mention spatial format"
        );
        Ok(())
    }
}
