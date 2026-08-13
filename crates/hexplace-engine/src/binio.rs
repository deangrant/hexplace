//! Safe little-endian reads and mmap open for columnar files.

use std::fs::File;
use std::path::Path;

use hexplace_core::CoreError;
use memmap2::Mmap;

/// Reads a little-endian `u32` at `off`, or returns a storage error.
pub(crate) fn u32_le(buf: &[u8], off: usize) -> Result<u32, CoreError> {
    let bytes: [u8; 4] = buf
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CoreError::storage("truncated u32 read"))?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads a little-endian `u64` at `off`, or returns a storage error.
pub(crate) fn u64_le(buf: &[u8], off: usize) -> Result<u64, CoreError> {
    let bytes: [u8; 8] = buf
        .get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CoreError::storage("truncated u64 read"))?;
    Ok(u64::from_le_bytes(bytes))
}

/// Reads a little-endian `i32` at `off`, or returns a storage error.
pub(crate) fn i32_le(buf: &[u8], off: usize) -> Result<i32, CoreError> {
    let bytes: [u8; 4] = buf
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CoreError::storage("truncated i32 read"))?;
    Ok(i32::from_le_bytes(bytes))
}

/// Reads a little-endian `f32` at `off`, or returns a storage error.
pub(crate) fn f32_le(buf: &[u8], off: usize) -> Result<f32, CoreError> {
    let bytes: [u8; 4] = buf
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CoreError::storage("truncated f32 read"))?;
    Ok(f32::from_le_bytes(bytes))
}

/// Optional little-endian `u32` (hot paths that skip on corruption).
pub(crate) fn u32_le_opt(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
}

/// Optional little-endian `u64` (hot paths that skip on corruption).
pub(crate) fn u64_le_opt(buf: &[u8], off: usize) -> Option<u64> {
    buf.get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
}

/// Optional little-endian `i32`.
pub(crate) fn i32_le_opt(buf: &[u8], off: usize) -> Option<i32> {
    buf.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(i32::from_le_bytes)
}

/// Memory-maps `path` and checks magic + version.
///
/// # Safety contract
///
/// Caller must not truncate or overwrite the file in place while mapped
/// (doing so can SIGBUS). Replace indexes via import publish and restart.
pub(crate) fn map_file(
    path: &Path,
    magic: &[u8; 4],
    version: u32,
    err: impl Fn(String) -> CoreError,
) -> Result<Mmap, CoreError> {
    let file = File::open(path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    let mmap =
        // SAFETY: see function contract above.
        unsafe { Mmap::map(&file) }.map_err(|e| CoreError::io(format!("mmap failed: {e}")))?;
    let header = mmap
        .get(..4)
        .ok_or_else(|| err(format!("{} bad header", path.display())))?;
    if mmap.len() < 16 || header != magic {
        return Err(err(format!("{} bad header", path.display())));
    }
    let file_version = match mmap.get(4..8).and_then(|s| <[u8; 4]>::try_from(s).ok()) {
        Some(bytes) => u32::from_le_bytes(bytes),
        None => return Err(err(format!("{} bad header", path.display()))),
    };
    if file_version != version {
        return Err(err(format!(
            "{} unsupported version {file_version}",
            path.display()
        )));
    }
    Ok(mmap)
}
