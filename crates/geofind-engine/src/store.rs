//! Memory-mapped place record store.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use geofind_core::{CoreError, Place, PlaceId, PlaceStore};
use memmap2::Mmap;

/// Magic bytes identifying a Geofind places file.
const MAGIC: &[u8; 4] = b"GFPL";
/// On-disk format version for `places.bin`.
const VERSION: u32 = 1;

/// Writes dense place records for later mmap reads.
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

    /// Writes `places.bin` with an offset table for O(1) id lookup.
    pub fn write_to(&self, path: &Path) -> Result<(), CoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut record_bytes = Vec::with_capacity(self.places.len());
        for place in &self.places {
            let encoded = serde_json::to_vec(place)
                .map_err(|e| CoreError::storage(format!("place encode failed: {e}")))?;
            if encoded.len() > u32::MAX as usize {
                return Err(CoreError::storage("place record too large"));
            }
            record_bytes.push(encoded);
        }

        let header_len = 16usize;
        let table_len = self.places.len() * 8;
        let mut offsets = Vec::with_capacity(self.places.len());
        let mut cursor = (header_len + table_len) as u64;
        for encoded in &record_bytes {
            offsets.push(cursor);
            cursor += 4 + encoded.len() as u64;
        }

        let file = File::create(path)?;
        let mut out = BufWriter::new(file);
        let count = self.places.len() as u64;
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&count.to_le_bytes())?;
        for off in &offsets {
            out.write_all(&off.to_le_bytes())?;
        }
        for encoded in &record_bytes {
            let len = encoded.len() as u32;
            out.write_all(&len.to_le_bytes())?;
            out.write_all(encoded)?;
        }
        out.flush()?;
        Ok(())
    }
}

impl Default for PlaceStoreWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory-mapped place store for serving.
pub struct MmapPlaceStore {
    mmap: Mmap,
    count: u64,
}

impl MmapPlaceStore {
    /// Opens an existing `places.bin` file.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let file = File::open(path)
            .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
        // SAFETY: the file is not mutated while mapped; imports replace files atomically.
        let mmap =
            unsafe { Mmap::map(&file) }.map_err(|e| CoreError::io(format!("mmap failed: {e}")))?;
        if mmap.len() < 16 {
            return Err(CoreError::storage("places.bin too small"));
        }
        if &mmap[0..4] != MAGIC {
            return Err(CoreError::storage("places.bin bad magic"));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version != VERSION {
            return Err(CoreError::storage(format!(
                "places.bin unsupported version {version}"
            )));
        }
        let count = u64::from_le_bytes(mmap[8..16].try_into().unwrap());
        let need = 16 + count as usize * 8;
        if mmap.len() < need {
            return Err(CoreError::storage("places.bin truncated offset table"));
        }
        Ok(Self { mmap, count })
    }

    /// Number of places in the store.
    pub fn len(&self) -> u64 {
        self.count
    }

    /// Returns true when the store has no places.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn offset_at(&self, id: PlaceId) -> Result<u64, CoreError> {
        if id >= self.count {
            return Err(CoreError::NotFound(id));
        }
        let start = 16 + (id as usize) * 8;
        Ok(u64::from_le_bytes(
            self.mmap[start..start + 8].try_into().unwrap(),
        ))
    }
}

impl PlaceStore for MmapPlaceStore {
    fn get(&self, id: PlaceId) -> Result<Place, CoreError> {
        let offset = self.offset_at(id)? as usize;
        if offset + 4 > self.mmap.len() {
            return Err(CoreError::storage("place offset out of range"));
        }
        let len = u32::from_le_bytes(self.mmap[offset..offset + 4].try_into().unwrap()) as usize;
        let start = offset + 4;
        let end = start + len;
        if end > self.mmap.len() {
            return Err(CoreError::storage("place record truncated"));
        }
        serde_json::from_slice(&self.mmap[start..end])
            .map_err(|e| CoreError::storage(format!("place decode failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geofind_core::{AddressParts, OsmType};

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
        let path = dir.path().join("places.bin");
        let mut w = PlaceStoreWriter::new();
        w.push(sample_place("Alpha"));
        w.push(sample_place("Beta"));
        w.write_to(&path).unwrap();

        let store = MmapPlaceStore::open(&path).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(0).unwrap().name.as_deref(), Some("Alpha"));
        assert_eq!(store.get(1).unwrap().name.as_deref(), Some("Beta"));
    }
}
