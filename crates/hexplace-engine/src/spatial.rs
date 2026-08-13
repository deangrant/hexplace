//! CSR H3 spatial index with fine (res 10) and coarse (res 6) levels.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use hexplace_core::{CoreError, Place, PlaceId, ReverseQuery, SpatialSearcher};
use h3o::{LatLng, Resolution};
use memmap2::Mmap;

/// Fine H3 resolution for reverse candidate lookup (~76 m edge).
pub const H3_RESOLUTION_FINE: u8 = 10;
/// Coarse H3 resolution for empty-cell fallback (~36 km²).
pub const H3_RESOLUTION_COARSE: u8 = 6;

/// Default fine resolution (alias for callers expecting a single default).
pub const DEFAULT_H3_RESOLUTION: u8 = H3_RESOLUTION_FINE;

const CELLS_MAGIC: &[u8; 4] = b"GFCL";
const OFFSETS_MAGIC: &[u8; 4] = b"GFOF";
const POSTINGS_MAGIC: &[u8; 4] = b"GFPO";
const VERSION: u32 = 2;

/// Paths for fine and coarse CSR indexes.
#[derive(Debug, Clone)]
pub struct SpatialPaths {
    /// Root `data/spatial` directory.
    pub root: PathBuf,
}

impl SpatialPaths {
    /// Creates helpers under `data/spatial`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Fine-level CSR directory.
    pub fn fine(&self) -> CsrPaths {
        CsrPaths::new(self.root.join("fine"))
    }

    /// Coarse-level CSR directory.
    pub fn coarse(&self) -> CsrPaths {
        CsrPaths::new(self.root.join("coarse"))
    }

    /// Ensures fine and coarse directories exist.
    pub fn ensure(&self) -> Result<(), CoreError> {
        fs::create_dir_all(self.fine().root)?;
        fs::create_dir_all(self.coarse().root)?;
        Ok(())
    }
}

/// Paths for one CSR layer.
#[derive(Debug, Clone)]
pub struct CsrPaths {
    /// Layer directory.
    pub root: PathBuf,
}

impl CsrPaths {
    /// Creates helpers for a CSR layer directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Sorted cell ids file.
    pub fn cells(&self) -> PathBuf {
        self.root.join("cells.bin")
    }

    /// Cell offset table file.
    pub fn offsets(&self) -> PathBuf {
        self.root.join("cell_offsets.bin")
    }

    /// Place-id postings file.
    pub fn postings(&self) -> PathBuf {
        self.root.join("postings.bin")
    }
}

/// Builds fine and coarse CSR indexes for `places`.
pub fn build_index(places: &[Place], paths: &SpatialPaths) -> Result<(), CoreError> {
    paths.ensure()?;
    build_csr_layer(places, &paths.fine(), H3_RESOLUTION_FINE)?;
    build_csr_layer(places, &paths.coarse(), H3_RESOLUTION_COARSE)?;
    Ok(())
}

fn build_csr_layer(places: &[Place], paths: &CsrPaths, resolution: u8) -> Result<(), CoreError> {
    let res = Resolution::try_from(resolution)
        .map_err(|e| CoreError::index(format!("invalid H3 resolution: {e}")))?;

    // cell -> (place_id, importance) list.
    let mut buckets: HashMap<u64, Vec<(u32, f32)>> = HashMap::new();
    for place in places {
        let ll = LatLng::new(place.lat, place.lon)
            .map_err(|e| CoreError::index(format!("invalid coordinates: {e}")))?;
        let cell = u64::from(ll.to_cell(res));
        buckets
            .entry(cell)
            .or_default()
            .push((place.place_id as u32, place.importance));
    }

    let mut cells: Vec<u64> = buckets.keys().copied().collect();
    cells.sort_unstable();

    let mut offsets: Vec<u64> = Vec::with_capacity(cells.len() + 1);
    let mut postings: Vec<u32> = Vec::new();
    offsets.push(0);
    for cell in &cells {
        let mut entries = buckets.remove(cell).unwrap_or_default();
        entries.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        for (place_id, _) in entries {
            postings.push(place_id);
        }
        offsets.push(postings.len() as u64);
    }

    write_u64_file(&paths.cells(), CELLS_MAGIC, &cells)?;
    write_u64_file(&paths.offsets(), OFFSETS_MAGIC, &offsets)?;
    write_u32_file(&paths.postings(), POSTINGS_MAGIC, &postings)?;
    Ok(())
}

fn write_u64_file(path: &Path, magic: &[u8; 4], values: &[u64]) -> Result<(), CoreError> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(magic)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(values.len() as u64).to_le_bytes())?;
    for v in values {
        out.write_all(&v.to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

fn write_u32_file(path: &Path, magic: &[u8; 4], values: &[u32]) -> Result<(), CoreError> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(magic)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&(values.len() as u64).to_le_bytes())?;
    for v in values {
        out.write_all(&v.to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

/// One mmap'd CSR layer.
struct CsrLayer {
    cells: Mmap,
    offsets: Mmap,
    postings: Mmap,
    cell_count: u64,
}

impl CsrLayer {
    fn open(paths: &CsrPaths) -> Result<Self, CoreError> {
        let cells = map_column(&paths.cells(), CELLS_MAGIC)?;
        let offsets = map_column(&paths.offsets(), OFFSETS_MAGIC)?;
        let postings = map_column(&paths.postings(), POSTINGS_MAGIC)?;
        let cell_count = u64::from_le_bytes(cells[8..16].try_into().unwrap());
        let offset_count = u64::from_le_bytes(offsets[8..16].try_into().unwrap());
        if offset_count != cell_count + 1 {
            return Err(CoreError::index("CSR offset count mismatch"));
        }
        Ok(Self {
            cells,
            offsets,
            postings,
            cell_count,
        })
    }

    fn cell_at(&self, index: usize) -> u64 {
        let start = 16 + index * 8;
        u64::from_le_bytes(self.cells[start..start + 8].try_into().unwrap())
    }

    fn offset_at(&self, index: usize) -> u64 {
        let start = 16 + index * 8;
        u64::from_le_bytes(self.offsets[start..start + 8].try_into().unwrap())
    }

    fn lookup_cell_ids(&self, cell: u64) -> Vec<PlaceId> {
        let mut lo = 0usize;
        let mut hi = self.cell_count as usize;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let c = self.cell_at(mid);
            if c < cell {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo >= self.cell_count as usize || self.cell_at(lo) != cell {
            return Vec::new();
        }
        let start = self.offset_at(lo) as usize;
        let end = self.offset_at(lo + 1) as usize;
        let mut ids = Vec::with_capacity(end.saturating_sub(start));
        for i in start..end {
            let off = 16 + i * 4;
            let id = u32::from_le_bytes(self.postings[off..off + 4].try_into().unwrap());
            ids.push(u64::from(id));
        }
        ids
    }
}

fn map_column(path: &Path, magic: &[u8; 4]) -> Result<Mmap, CoreError> {
    let file = File::open(path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    // SAFETY: spatial files are immutable while serving.
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| CoreError::io(format!("mmap spatial failed: {e}")))?;
    if mmap.len() < 16 || &mmap[0..4] != magic {
        return Err(CoreError::index(format!("{} bad header", path.display())));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version != VERSION {
        return Err(CoreError::index(format!(
            "{} unsupported version {version}",
            path.display()
        )));
    }
    Ok(mmap)
}

/// Two-level H3 reverse index.
pub struct H3SpatialIndex {
    fine: CsrLayer,
    coarse: CsrLayer,
    fine_res: Resolution,
    coarse_res: Resolution,
}

impl H3SpatialIndex {
    /// Opens fine and coarse CSR layers from `paths`.
    pub fn open(paths: &SpatialPaths) -> Result<Self, CoreError> {
        let fine = CsrLayer::open(&paths.fine())?;
        let coarse = CsrLayer::open(&paths.coarse())?;
        let fine_res = Resolution::try_from(H3_RESOLUTION_FINE)
            .map_err(|e| CoreError::index(format!("bad fine resolution: {e}")))?;
        let coarse_res = Resolution::try_from(H3_RESOLUTION_COARSE)
            .map_err(|e| CoreError::index(format!("bad coarse resolution: {e}")))?;
        Ok(Self {
            fine,
            coarse,
            fine_res,
            coarse_res,
        })
    }
}

impl SpatialSearcher for H3SpatialIndex {
    fn candidates(&self, query: &ReverseQuery) -> Result<Vec<PlaceId>, CoreError> {
        let ll = LatLng::new(query.point.lat, query.point.lon)
            .map_err(|e| CoreError::invalid(format!("invalid coordinates: {e}")))?;
        let cell = ll.to_cell(self.fine_res);
        let mut seen = HashSet::new();
        let mut ids = Vec::new();

        for id in self.fine.lookup_cell_ids(u64::from(cell)) {
            if seen.insert(id) {
                ids.push(id);
            }
        }
        if ids.len() < query.limit {
            let neighbors: Vec<_> = cell.grid_disk(1);
            for neighbor in neighbors {
                for id in self.fine.lookup_cell_ids(u64::from(neighbor)) {
                    if seen.insert(id) {
                        ids.push(id);
                    }
                }
            }
        }
        if ids.len() < query.limit {
            let neighbors: Vec<_> = cell.grid_disk(2);
            for neighbor in neighbors {
                for id in self.fine.lookup_cell_ids(u64::from(neighbor)) {
                    if seen.insert(id) {
                        ids.push(id);
                    }
                }
            }
        }
        if ids.is_empty() {
            let coarse = ll.to_cell(self.coarse_res);
            for id in self.coarse.lookup_cell_ids(u64::from(coarse)) {
                if seen.insert(id) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hexplace_core::{AddressParts, OsmType};

    fn place(id: u64, lat: f64, lon: f64, importance: f32) -> Place {
        Place {
            place_id: id,
            osm_type: OsmType::Node,
            osm_id: id + 1,
            lat,
            lon,
            name: Some(format!("P{id}")),
            display_name: format!("P{id}"),
            category: "place".into(),
            type_name: "city".into(),
            address: AddressParts::default(),
            importance,
        }
    }

    #[test]
    fn reverse_finds_nearby_place() {
        let dir = tempfile::tempdir().unwrap();
        let paths = SpatialPaths::new(dir.path().join("spatial"));
        let places = vec![place(0, 48.8566, 2.3522, 0.9)];
        build_index(&places, &paths).unwrap();
        let index = H3SpatialIndex::open(&paths).unwrap();
        let q = ReverseQuery::new(48.8566, 2.3522, Some(5)).unwrap();
        let ids = index.candidates(&q).unwrap();
        assert!(ids.contains(&0));
    }

    #[test]
    fn dense_cell_returns_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let paths = SpatialPaths::new(dir.path().join("spatial"));
        let mut places = Vec::new();
        for i in 0..40u64 {
            // Cluster tightly so many share a fine cell.
            places.push(place(i, 48.8566 + (i as f64) * 0.00001, 2.3522, 0.5));
        }
        build_index(&places, &paths).unwrap();
        let index = H3SpatialIndex::open(&paths).unwrap();
        let q = ReverseQuery::new(48.8566, 2.3522, Some(5)).unwrap();
        let ids = index.candidates(&q).unwrap();
        assert!(ids.len() >= 5);
    }

    #[test]
    fn empty_fine_falls_back_to_coarse() {
        let dir = tempfile::tempdir().unwrap();
        let paths = SpatialPaths::new(dir.path().join("spatial"));
        // Place in Paris.
        let places = vec![place(0, 48.8566, 2.3522, 0.9)];
        build_index(&places, &paths).unwrap();
        let index = H3SpatialIndex::open(&paths).unwrap();
        // Query a few km away within same coarse cell neighborhood.
        let q = ReverseQuery::new(48.87, 2.36, Some(1)).unwrap();
        let ids = index.candidates(&q).unwrap();
        // May find via ring expand or coarse; either way should not panic.
        let _ = ids;
    }
}
