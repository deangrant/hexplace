//! Columnar zero-copy place store.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use hexplace_core::{AddressParts, CoreError, OsmType, Place, PlaceId, PlaceStore};
use memmap2::Mmap;

const COORDS_MAGIC: &[u8; 4] = b"GFCO";
const META_MAGIC: &[u8; 4] = b"GFMT";
const STRINGS_MAGIC: &[u8; 4] = b"GFST";
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
    pub fn coords(&self) -> PathBuf {
        self.root.join("coords.bin")
    }

    /// Fixed metadata column file.
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta.bin")
    }

    /// Concatenated string blob.
    pub fn strings(&self) -> PathBuf {
        self.root.join("strings.bin")
    }

    /// Ensures the places directory exists.
    pub fn ensure(&self) -> Result<(), CoreError> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

/// Writes dense columnar place records.
pub struct PlaceStoreWriter {
    places: Vec<Place>,
}

impl PlaceStoreWriter {
    /// Creates an empty writer.
    pub fn new() -> Self {
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
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Returns true when no places have been added.
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Borrows the buffered places.
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// Writes columnar files under `paths`.
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
    (v * 10_000_000.0).round() as i32
}

fn from_e7(v: i32) -> f64 {
    f64::from(v) / 10_000_000.0
}

fn write_coords(path: &Path, places: &[Place]) -> Result<(), CoreError> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(COORDS_MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(places.len() as u64).to_le_bytes())?;
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
    meta.write_all(META_MAGIC)?;
    meta.write_all(&VERSION.to_le_bytes())?;
    meta.write_all(&(places.len() as u64).to_le_bytes())?;

    let strings_file = File::create(strings_path)?;
    let mut strings = BufWriter::new(strings_file);
    strings.write_all(STRINGS_MAGIC)?;
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
        meta.write_all(&(encoded.len() as u32).to_le_bytes())?;
        strings.write_all(&encoded)?;
        cursor += encoded.len() as u64;
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

impl MmapPlaceStore {
    /// Opens a columnar place store directory.
    pub fn open(paths: &PlacePaths) -> Result<Self, CoreError> {
        let coords = map_file(&paths.coords(), COORDS_MAGIC)?;
        let meta = map_file(&paths.meta(), META_MAGIC)?;
        let strings = map_file(&paths.strings(), STRINGS_MAGIC)?;
        let count = u64::from_le_bytes(coords[8..16].try_into().unwrap());
        let meta_count = u64::from_le_bytes(meta[8..16].try_into().unwrap());
        if count != meta_count {
            return Err(CoreError::storage("coords/meta count mismatch"));
        }
        let coords_need = 16 + count as usize * COORD_STRIDE;
        let meta_need = 16 + count as usize * META_STRIDE;
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
    pub fn len(&self) -> u64 {
        self.count
    }

    /// Returns true when the store has no places.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn check_id(&self, id: PlaceId) -> Result<(), CoreError> {
        if id >= self.count {
            Err(CoreError::NotFound(id))
        } else {
            Ok(())
        }
    }
}

fn map_file(path: &Path, magic: &[u8; 4]) -> Result<Mmap, CoreError> {
    let file = File::open(path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    // SAFETY: caller must not truncate or overwrite these files in place while
    // mapped (doing so can SIGBUS). Replace indexes via import publish and
    // restart the server to load the new data directory.
    let mmap =
        unsafe { Mmap::map(&file) }.map_err(|e| CoreError::io(format!("mmap failed: {e}")))?;
    if mmap.len() < 16 || &mmap[0..4] != magic {
        return Err(CoreError::storage(format!("{} bad header", path.display())));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version != VERSION {
        return Err(CoreError::storage(format!(
            "{} unsupported version {version}",
            path.display()
        )));
    }
    Ok(mmap)
}

impl PlaceStore for MmapPlaceStore {
    fn get(&self, id: PlaceId) -> Result<Place, CoreError> {
        self.check_id(id)?;
        let (lat, lon) = self.coord(id)?;
        let importance = self.importance(id)?;
        let meta_off = 16 + id as usize * META_STRIDE;
        let osm_type = match self.meta[meta_off] {
            0 => OsmType::Node,
            1 => OsmType::Way,
            _ => OsmType::Relation,
        };
        let osm_id = u64::from_le_bytes(self.meta[meta_off + 5..meta_off + 13].try_into().unwrap());
        let str_off =
            u64::from_le_bytes(self.meta[meta_off + 13..meta_off + 21].try_into().unwrap())
                as usize;
        let str_len =
            u32::from_le_bytes(self.meta[meta_off + 21..meta_off + 25].try_into().unwrap())
                as usize;
        if str_off + str_len > self.strings.len() {
            return Err(CoreError::storage("string blob out of range"));
        }
        let blob = &self.strings[str_off..str_off + str_len];
        let parts = split_cstrings(blob)?;
        if parts.len() != 5 {
            return Err(CoreError::storage("string blob field count mismatch"));
        }
        let name = if parts[0].is_empty() {
            None
        } else {
            Some(parts[0].to_owned())
        };
        let address: AddressParts = serde_json::from_str(parts[4])
            .map_err(|e| CoreError::storage(format!("address decode failed: {e}")))?;
        Ok(Place {
            place_id: id,
            osm_type,
            osm_id,
            lat,
            lon,
            name,
            display_name: parts[1].to_owned(),
            category: parts[2].to_owned(),
            type_name: parts[3].to_owned(),
            address,
            importance,
        })
    }

    fn coord(&self, id: PlaceId) -> Result<(f64, f64), CoreError> {
        self.check_id(id)?;
        let off = 16 + id as usize * COORD_STRIDE;
        let lat = i32::from_le_bytes(self.coords[off..off + 4].try_into().unwrap());
        let lon = i32::from_le_bytes(self.coords[off + 4..off + 8].try_into().unwrap());
        Ok((from_e7(lat), from_e7(lon)))
    }

    fn importance(&self, id: PlaceId) -> Result<f32, CoreError> {
        self.check_id(id)?;
        let off = 16 + id as usize * META_STRIDE + 1;
        Ok(f32::from_le_bytes(
            self.meta[off..off + 4].try_into().unwrap(),
        ))
    }
}

fn split_cstrings(blob: &[u8]) -> Result<Vec<&str>, CoreError> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for i in 0..blob.len() {
        if blob[i] == 0 {
            let s = std::str::from_utf8(&blob[start..i])
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
mod tests {
    use super::*;
    use hexplace_core::{AddressParts, OsmType};

    fn sample_place(name: &str) -> Place {
        Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 1,
            lat: 48.0,
            lon: 2.0,
            name: Some(name.to_owned()),
            display_name: name.to_owned(),
            category: "place".to_owned(),
            type_name: "city".to_owned(),
            address: AddressParts::default(),
            importance: 0.5,
        }
    }

    #[test]
    fn round_trip_places() {
        let dir = tempfile::tempdir().unwrap();
        let paths = PlacePaths::new(dir.path().join("places"));
        let mut w = PlaceStoreWriter::new();
        w.push(sample_place("Alpha"));
        w.push(sample_place("Beta"));
        w.write_to(&paths).unwrap();

        let store = MmapPlaceStore::open(&paths).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(0).unwrap().name.as_deref(), Some("Alpha"));
        assert_eq!(store.get(1).unwrap().name.as_deref(), Some("Beta"));
        let (lat, lon) = store.coord(0).unwrap();
        assert!((lat - 48.0).abs() < 1e-6);
        assert!((lon - 2.0).abs() < 1e-6);
        assert_eq!(store.importance(0).unwrap(), 0.5);
    }

    #[test]
    fn encode_rejects_embedded_nul() {
        let mut place = sample_place("ok");
        place.name = Some("bad\0name".into());
        let err = encode_strings(&place).unwrap_err();
        assert!(err.to_string().contains("NUL"));

        place.name = Some("ok".into());
        place.display_name = "bad\0display".into();
        let err = encode_strings(&place).unwrap_err();
        assert!(err.to_string().contains("NUL"));
    }

    #[test]
    fn split_cstrings_rejects_malformed_blobs() {
        let missing_final = b"a\0b\0c\0d\0e";
        let err = split_cstrings(missing_final).unwrap_err();
        assert!(err.to_string().contains("missing final NUL"));

        let too_few = b"a\0b\0c\0";
        let parts = split_cstrings(too_few).unwrap();
        assert_ne!(parts.len(), 5);

        let exact = b"a\0b\0c\0d\0{}\0";
        let parts = split_cstrings(exact).unwrap();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[4], "{}");
    }
}
