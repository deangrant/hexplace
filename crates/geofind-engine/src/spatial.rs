//! H3 spatial postings for reverse geocoding.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use geofind_core::{CoreError, Place, PlaceId, ReverseQuery, SpatialSearcher};
use h3o::{LatLng, Resolution};
use memmap2::Mmap;

/// Default H3 resolution for reverse candidate lookup.
pub const DEFAULT_H3_RESOLUTION: u8 = 9;

const MAGIC: &[u8; 4] = b"GFH3";
const VERSION: u32 = 1;

/// Builds a sorted `(cell, place_id)` postings file.
pub fn build_index(places: &[Place], path: &Path, resolution: u8) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let res = Resolution::try_from(resolution)
        .map_err(|e| CoreError::index(format!("invalid H3 resolution: {e}")))?;

    let mut pairs: Vec<(u64, u64)> = Vec::with_capacity(places.len() * 7);
    for place in places {
        let ll = LatLng::new(place.lat, place.lon)
            .map_err(|e| CoreError::index(format!("invalid coordinates: {e}")))?;
        let cell = ll.to_cell(res);
        pairs.push((u64::from(cell), place.place_id));
        // Index the hex ring so nearby queries still find the place.
        let neighbors: Vec<_> = cell.grid_disk(1);
        for neighbor in neighbors {
            pairs.push((u64::from(neighbor), place.place_id));
        }
    }
    pairs.sort_unstable();
    pairs.dedup();

    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(resolution as u32).to_le_bytes())?;
    out.write_all(&(pairs.len() as u64).to_le_bytes())?;
    for (cell, place_id) in pairs {
        out.write_all(&cell.to_le_bytes())?;
        out.write_all(&place_id.to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

/// Memory-mapped H3 postings searcher.
pub struct H3SpatialIndex {
    mmap: Mmap,
    resolution: Resolution,
    count: u64,
}

impl H3SpatialIndex {
    /// Opens an H3 postings file.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let file = File::open(path)
            .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
        // SAFETY: spatial file is immutable while the server runs.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| CoreError::io(format!("mmap spatial failed: {e}")))?;
        if mmap.len() < 20 {
            return Err(CoreError::index("spatial index too small"));
        }
        if &mmap[0..4] != MAGIC {
            return Err(CoreError::index("spatial index bad magic"));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version != VERSION {
            return Err(CoreError::index(format!(
                "spatial index unsupported version {version}"
            )));
        }
        let resolution = u32::from_le_bytes(mmap[8..12].try_into().unwrap()) as u8;
        let res = Resolution::try_from(resolution)
            .map_err(|e| CoreError::index(format!("bad stored resolution: {e}")))?;
        let count = u64::from_le_bytes(mmap[12..20].try_into().unwrap());
        let need = 20 + count as usize * 16;
        if mmap.len() < need {
            return Err(CoreError::index("spatial index truncated"));
        }
        Ok(Self {
            mmap,
            resolution: res,
            count,
        })
    }

    fn pair_at(&self, index: usize) -> (u64, u64) {
        let start = 20 + index * 16;
        let cell = u64::from_le_bytes(self.mmap[start..start + 8].try_into().unwrap());
        let place_id = u64::from_le_bytes(self.mmap[start + 8..start + 16].try_into().unwrap());
        (cell, place_id)
    }

    fn lookup_cell(&self, cell: u64) -> Vec<PlaceId> {
        // Binary search for the first matching cell.
        let mut lo = 0usize;
        let mut hi = self.count as usize;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (c, _) = self.pair_at(mid);
            if c < cell {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut ids = Vec::new();
        let mut i = lo;
        while i < self.count as usize {
            let (c, place_id) = self.pair_at(i);
            if c != cell {
                break;
            }
            ids.push(place_id);
            i += 1;
        }
        ids
    }
}

impl SpatialSearcher for H3SpatialIndex {
    fn candidates(&self, query: &ReverseQuery) -> Result<Vec<PlaceId>, CoreError> {
        let ll = LatLng::new(query.point.lat, query.point.lon)
            .map_err(|e| CoreError::invalid(format!("invalid coordinates: {e}")))?;
        let cell = ll.to_cell(self.resolution);
        let mut ids = self.lookup_cell(u64::from(cell));
        if ids.len() < query.limit {
            let neighbors: Vec<_> = cell.grid_disk(1);
            for neighbor in neighbors {
                for id in self.lookup_cell(u64::from(neighbor)) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        if ids.len() < query.limit {
            let neighbors: Vec<_> = cell.grid_disk(2);
            for neighbor in neighbors {
                for id in self.lookup_cell(u64::from(neighbor)) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geofind_core::{AddressParts, OsmType};

    #[test]
    fn reverse_finds_nearby_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h3.bin");
        let places = vec![Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 1,
            lat: 48.8566,
            lon: 2.3522,
            name: Some("Paris".into()),
            display_name: "Paris".into(),
            category: "place".into(),
            type_name: "city".into(),
            address: AddressParts::default(),
            importance: 0.9,
        }];
        build_index(&places, &path, DEFAULT_H3_RESOLUTION).unwrap();
        let index = H3SpatialIndex::open(&path).unwrap();
        let q = ReverseQuery::new(48.8566, 2.3522, Some(5)).unwrap();
        let ids = index.candidates(&q).unwrap();
        assert!(ids.contains(&0));
    }
}
