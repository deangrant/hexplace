//! Columnar zero-copy place store.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use hexplace_core::{AddressParts, CoreError, OsmType, Place, PlaceId, PlaceStore};
use memmap2::Mmap;

use crate::binio::{self, f32_le, i32_le, u32_le, u64_le};

const COORDS_MAGIC: [u8; 4] = *b"GFCO";
const META_MAGIC: [u8; 4] = *b"GFMT";
const STRINGS_MAGIC: [u8; 4] = *b"GFST";
const VERSION: u32 = 2;
const COORD_STRIDE: usize = 8;
const META_STRIDE: usize = 1 + 4 + 8 + 8 + 4; // 25 bytes

/// Paths for the columnar place store.
#[derive(Debug, Clone)]
pub struct PlacePaths {
    /// Directory containing coords/meta/strings.
    pub root: PathBuf,
}

impl PlacePaths {
    /// Creates path helpers under `data/places`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Coordinate column file.
    #[must_use]
    pub fn coords(&self) -> PathBuf {
        self.root.join("coords.bin")
    }

    /// Fixed metadata column file.
    #[must_use]
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta.bin")
    }

    /// Concatenated string blob.
    #[must_use]
    pub fn strings(&self) -> PathBuf {
        self.root.join("strings.bin")
    }

    /// Ensures the places directory exists.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the directory cannot be created.
    pub fn ensure(&self) -> Result<(), CoreError> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

/// Writes dense columnar place records.
#[derive(Debug)]
pub struct PlaceStoreWriter {
    places: Vec<Place>,
}

impl PlaceStoreWriter {
    /// Creates an empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { places: Vec::new() }
    }

    /// Appends a place, assigning the next dense id.
    pub fn push(&mut self, mut place: Place) -> PlaceId {
        let id = self.places.len() as PlaceId;
        place.place_id = id;
        self.places.push(place);
        id
    }

    /// Returns the number of places buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Returns true when no places have been added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Borrows the buffered places.
    #[must_use]
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// Writes columnar files under `paths`.
    ///
    /// # Errors
    ///
    /// Returns a storage or I/O error when encoding fails or files cannot be
    /// written.
    pub fn write_to(&self, paths: &PlacePaths) -> Result<(), CoreError> {
        paths.ensure()?;
        write_coords(&paths.coords(), &self.places)?;
        write_meta_and_strings(&paths.meta(), &paths.strings(), &self.places)?;
        Ok(())
    }
}

impl Default for PlaceStoreWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn to_e7(v: f64) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "WGS84 degrees scaled to OSM 1e-7 units fit i32"
    )]
    {
        (v * 10_000_000.0).round() as i32
    }
}

fn from_e7(v: i32) -> f64 {
    f64::from(v) / 10_000_000.0
}

fn write_coords(path: &Path, places: &[Place]) -> Result<(), CoreError> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(&COORDS_MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    let len =
        u64::try_from(places.len()).map_err(|_| CoreError::storage("place count exceeds u64"))?;
    out.write_all(&len.to_le_bytes())?;
    for place in places {
        out.write_all(&to_e7(place.lat).to_le_bytes())?;
        out.write_all(&to_e7(place.lon).to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

fn push_cstring(buf: &mut Vec<u8>, s: &str) -> Result<(), CoreError> {
    if s.as_bytes().contains(&0) {
        return Err(CoreError::storage("string field contains NUL"));
    }
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
    Ok(())
}

fn encode_strings(place: &Place) -> Result<Vec<u8>, CoreError> {
    let name = place.name.as_deref().unwrap_or("");
    let address = serde_json::to_string(&place.address)
        .map_err(|e| CoreError::storage(format!("address encode failed: {e}")))?;
    let mut buf = Vec::new();
    push_cstring(&mut buf, name)?;
    push_cstring(&mut buf, &place.display_name)?;
    push_cstring(&mut buf, &place.category)?;
    push_cstring(&mut buf, &place.type_name)?;
    push_cstring(&mut buf, &address)?;
    Ok(buf)
}

fn write_meta_and_strings(
    meta_path: &Path,
    strings_path: &Path,
    places: &[Place],
) -> Result<(), CoreError> {
    let meta_file = File::create(meta_path)?;
    let mut meta = BufWriter::new(meta_file);
    meta.write_all(&META_MAGIC)?;
    meta.write_all(&VERSION.to_le_bytes())?;
    let len =
        u64::try_from(places.len()).map_err(|_| CoreError::storage("place count exceeds u64"))?;
    meta.write_all(&len.to_le_bytes())?;

    let strings_file = File::create(strings_path)?;
    let mut strings = BufWriter::new(strings_file);
    strings.write_all(&STRINGS_MAGIC)?;
    strings.write_all(&VERSION.to_le_bytes())?;
    // strings.bin: magic(4) + version(4) + concatenated NUL-terminated blobs.
    // Meta str_off is the absolute file offset of each blob; cursor starts at 8.
    let mut cursor = 8u64;

    for place in places {
        let encoded = encode_strings(place)?;
        let osm_type = match place.osm_type {
            OsmType::Node => 0u8,
            OsmType::Way => 1u8,
            OsmType::Relation => 2u8,
        };
        meta.write_all(&[osm_type])?;
        meta.write_all(&place.importance.to_le_bytes())?;
        meta.write_all(&place.osm_id.to_le_bytes())?;
        meta.write_all(&cursor.to_le_bytes())?;
        let encoded_len = u32::try_from(encoded.len())
            .map_err(|_| CoreError::storage("string blob exceeds u32 length"))?;
        meta.write_all(&encoded_len.to_le_bytes())?;
        strings.write_all(&encoded)?;
        cursor = cursor
            .checked_add(u64::from(encoded_len))
            .ok_or_else(|| CoreError::storage("string cursor overflow"))?;
    }
    meta.flush()?;
    strings.flush()?;
    Ok(())
}

/// Memory-mapped columnar place store.
pub struct MmapPlaceStore {
    coords: Mmap,
    meta: Mmap,
    strings: Mmap,
    count: u64,
}

impl std::fmt::Debug for MmapPlaceStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MmapPlaceStore")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

impl MmapPlaceStore {
    /// Opens a columnar place store directory.
    ///
    /// # Errors
    ///
    /// Returns a storage or I/O error when a column is missing, truncated, or
    /// the coords/meta counts disagree.
    pub fn open(paths: &PlacePaths) -> Result<Self, CoreError> {
        let coords = binio::map_file(&paths.coords(), COORDS_MAGIC, VERSION, |m| {
            CoreError::storage(m)
        })?;
        let meta = binio::map_file(&paths.meta(), META_MAGIC, VERSION, |m| {
            CoreError::storage(m)
        })?;
        let strings = binio::map_file(&paths.strings(), STRINGS_MAGIC, VERSION, |m| {
            CoreError::storage(m)
        })?;
        let count = u64_le(&coords, 8)?;
        let meta_count = u64_le(&meta, 8)?;
        if count != meta_count {
            return Err(CoreError::storage("coords/meta count mismatch"));
        }
        let count_usize = usize::try_from(count)
            .map_err(|_| CoreError::storage("place count too large for usize"))?;
        let coords_need = 16usize
            .checked_add(
                count_usize
                    .checked_mul(COORD_STRIDE)
                    .ok_or_else(|| CoreError::storage("coords size overflow"))?,
            )
            .ok_or_else(|| CoreError::storage("coords size overflow"))?;
        let meta_need = 16usize
            .checked_add(
                count_usize
                    .checked_mul(META_STRIDE)
                    .ok_or_else(|| CoreError::storage("meta size overflow"))?,
            )
            .ok_or_else(|| CoreError::storage("meta size overflow"))?;
        if coords.len() < coords_need || meta.len() < meta_need {
            return Err(CoreError::storage("place columns truncated"));
        }
        if strings.len() < 8 {
            return Err(CoreError::storage("strings.bin too small"));
        }
        Ok(Self {
            coords,
            meta,
            strings,
            count,
        })
    }

    /// Number of places in the store.
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.count
    }

    /// Returns true when the store has no places.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    const fn check_id(&self, id: PlaceId) -> Result<(), CoreError> {
        if id >= self.count {
            Err(CoreError::NotFound(id))
        } else {
            Ok(())
        }
    }

    fn row_offset(id: PlaceId, stride: usize) -> Result<usize, CoreError> {
        let id = usize::try_from(id).map_err(|_| CoreError::storage("place id too large"))?;
        16usize
            .checked_add(
                id.checked_mul(stride)
                    .ok_or_else(|| CoreError::storage("place row offset overflow"))?,
            )
            .ok_or_else(|| CoreError::storage("place row offset overflow"))
    }
}

impl PlaceStore for MmapPlaceStore {
    fn get(&self, id: PlaceId) -> Result<Place, CoreError> {
        self.check_id(id)?;
        let (lat, lon) = self.coord(id)?;
        let importance = self.importance(id)?;
        let meta_off = Self::row_offset(id, META_STRIDE)?;
        let osm_type = match self.meta.get(meta_off).copied() {
            Some(0) => OsmType::Node,
            Some(1) => OsmType::Way,
            Some(_) => OsmType::Relation,
            None => return Err(CoreError::storage("meta row truncated")),
        };
        let osm_id = u64_le(&self.meta, meta_off + 5)?;
        let str_off = usize::try_from(u64_le(&self.meta, meta_off + 13)?)
            .map_err(|_| CoreError::storage("string offset too large"))?;
        let str_len = usize::try_from(u32_le(&self.meta, meta_off + 21)?)
            .map_err(|_| CoreError::storage("string length too large"))?;
        let end = str_off
            .checked_add(str_len)
            .ok_or_else(|| CoreError::storage("string blob out of range"))?;
        let blob = self
            .strings
            .get(str_off..end)
            .ok_or_else(|| CoreError::storage("string blob out of range"))?;
        let parts = split_cstrings(blob)?;
        let [name_s, display_name, category, type_name, address_json] =
            <[&str; 5]>::try_from(parts.as_slice())
                .map_err(|_| CoreError::storage("string blob field count mismatch"))?;
        let name = if name_s.is_empty() {
            None
        } else {
            Some(name_s.to_owned())
        };
        let address: AddressParts = serde_json::from_str(address_json)
            .map_err(|e| CoreError::storage(format!("address decode failed: {e}")))?;
        Ok(Place {
            place_id: id,
            osm_type,
            osm_id,
            lat,
            lon,
            name,
            display_name: display_name.to_owned(),
            category: category.to_owned(),
            type_name: type_name.to_owned(),
            address,
            importance,
        })
    }

    fn coord(&self, id: PlaceId) -> Result<(f64, f64), CoreError> {
        self.check_id(id)?;
        let off = Self::row_offset(id, COORD_STRIDE)?;
        let lat = i32_le(&self.coords, off)?;
        let lon = i32_le(&self.coords, off + 4)?;
        Ok((from_e7(lat), from_e7(lon)))
    }

    fn importance(&self, id: PlaceId) -> Result<f32, CoreError> {
        self.check_id(id)?;
        let off = Self::row_offset(id, META_STRIDE)? + 1;
        f32_le(&self.meta, off)
    }
}

fn split_cstrings(blob: &[u8]) -> Result<Vec<&str>, CoreError> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for i in 0..blob.len() {
        if blob.get(i).copied() == Some(0) {
            let s = std::str::from_utf8(
                blob.get(start..i)
                    .ok_or_else(|| CoreError::storage("string blob slice out of range"))?,
            )
            .map_err(|e| CoreError::storage(format!("utf8 decode failed: {e}")))?;
            out.push(s);
            start = i + 1;
        }
    }
    if start != blob.len() {
        return Err(CoreError::storage("string blob missing final NUL"));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
