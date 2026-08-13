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

    // Postings are intentionally 32-bit until a future spatial format bump.
    if places.len() > u32::MAX as usize {
        return Err(CoreError::index(
            "place count exceeds u32; spatial postings are 32-bit",
        ));
    }

    // cell -> (place_id, importance) list.
    let mut buckets: HashMap<u64, Vec<(u32, f32)>> = HashMap::new();
    for place in places {
        let ll = LatLng::new(place.lat, place.lon)
            .map_err(|e| CoreError::index(format!("invalid coordinates: {e}")))?;
        let cell = u64::from(ll.to_cell(res));
        let place_id = u32::try_from(place.place_id).map_err(|_| {
            CoreError::index(format!(
                "place_id {} exceeds u32; spatial postings are 32-bit",
                place.place_id
            ))
        })?;
        buckets
            .entry(cell)
            .or_default()
            .push((place_id, place.importance));
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
///
/// After [`CsrLayer::open`] succeeds, `cell_at` / `offset_at` / posting reads
/// assume the mapped columns cover the claimed arrays and that offsets are
/// monotone and within the postings length.
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
        let cell_count = header_count(&cells);
        let offset_count = header_count(&offsets);
        let posting_count = header_count(&postings);
        let expected_offsets = cell_count
            .checked_add(1)
            .ok_or_else(|| CoreError::index("CSR cell count overflow"))?;
        if offset_count != expected_offsets {
            return Err(CoreError::index("CSR offset count mismatch"));
        }
        let cells_need = column_byte_len(cell_count, 8)?;
        let offsets_need = column_byte_len(offset_count, 8)?;
        let postings_need = column_byte_len(posting_count, 4)?;
        if cells.len() < cells_need
            || offsets.len() < offsets_need
            || postings.len() < postings_need
        {
            return Err(CoreError::index("CSR columns truncated"));
        }
        validate_offset_table(&offsets, offset_count, posting_count)?;
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

fn header_count(mmap: &Mmap) -> u64 {
    u64::from_le_bytes(mmap[8..16].try_into().unwrap())
}

fn column_byte_len(count: u64, stride: usize) -> Result<usize, CoreError> {
    let count = usize::try_from(count)
        .map_err(|_| CoreError::index("CSR column count too large"))?;
    let body = count
        .checked_mul(stride)
        .ok_or_else(|| CoreError::index("CSR column size overflow"))?;
    body.checked_add(16)
        .ok_or_else(|| CoreError::index("CSR column size overflow"))
}

fn u64_at(mmap: &Mmap, index: u64) -> u64 {
    let start = 16 + index as usize * 8;
    u64::from_le_bytes(mmap[start..start + 8].try_into().unwrap())
}

fn validate_offset_table(
    offsets: &Mmap,
    offset_count: u64,
    posting_count: u64,
) -> Result<(), CoreError> {
    let first = u64_at(offsets, 0);
    if first != 0 {
        return Err(CoreError::index("CSR offsets must start at 0"));
    }
    let last = u64_at(offsets, offset_count - 1);
    if last != posting_count {
        return Err(CoreError::index("CSR offsets last entry mismatch"));
    }
    let mut prev = first;
    for i in 1..offset_count {
        let cur = u64_at(offsets, i);
        if cur < prev || cur > posting_count {
            return Err(CoreError::index("CSR offsets not monotone"));
        }
        prev = cur;
    }
    Ok(())
}

fn map_column(path: &Path, magic: &[u8; 4]) -> Result<Mmap, CoreError> {
    let file = File::open(path)
        .map_err(|e| CoreError::io(format!("failed to open {}: {e}", path.display())))?;
    // SAFETY: caller must not truncate or overwrite these files in place while
    // mapped (doing so can SIGBUS). Replace indexes via a new data directory
    // and restart the server.
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
        for k in 1..=2 {
            if ids.len() >= query.limit {
                break;
            }
            for neighbor in cell.grid_ring_fast(k).flatten() {
                for id in self.fine.lookup_cell_ids(u64::from(neighbor)) {
                    if seen.insert(id) {
                        ids.push(id);
                    }
                }
            }
        }
        if ids.len() < query.limit {
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
        // Place in Paris — present in both fine and coarse after build.
        let places = vec![place(0, 48.8566, 2.3522, 0.9)];
        build_index(&places, &paths).unwrap();

        // Empty the fine layer so candidates must come from coarse.
        let fine = paths.fine();
        write_u64_file(&fine.cells(), CELLS_MAGIC, &[]).unwrap();
        write_u64_file(&fine.offsets(), OFFSETS_MAGIC, &[0]).unwrap();
        write_u32_file(&fine.postings(), POSTINGS_MAGIC, &[]).unwrap();

        let index = H3SpatialIndex::open(&paths).unwrap();
        let q = ReverseQuery::new(48.8566, 2.3522, Some(1)).unwrap();
        let ids = index.candidates(&q).unwrap();
        assert!(
            ids.contains(&0),
            "expected coarse fallback to return place 0, got {ids:?}"
        );
    }

    fn build_sample_index() -> (tempfile::TempDir, SpatialPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = SpatialPaths::new(dir.path().join("spatial"));
        let places = vec![place(0, 48.8566, 2.3522, 0.9)];
        build_index(&places, &paths).unwrap();
        (dir, paths)
    }

    fn truncate_path(path: &Path, len: u64) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_len(len).unwrap();
    }

    #[test]
    fn open_rejects_truncated_cells() {
        let (_dir, paths) = build_sample_index();
        let cells = paths.fine().cells();
        let len = std::fs::metadata(&cells).unwrap().len();
        truncate_path(&cells, len - 1);
        assert!(H3SpatialIndex::open(&paths).is_err());
    }

    #[test]
    fn open_rejects_truncated_postings() {
        let (_dir, paths) = build_sample_index();
        let postings = paths.fine().postings();
        let len = std::fs::metadata(&postings).unwrap().len();
        truncate_path(&postings, len - 1);
        assert!(H3SpatialIndex::open(&paths).is_err());
    }

    #[test]
    fn open_rejects_non_monotone_offsets() {
        let (_dir, paths) = build_sample_index();
        write_u64_file(&paths.fine().cells(), CELLS_MAGIC, &[1, 2, 3]).unwrap();
        write_u64_file(&paths.fine().offsets(), OFFSETS_MAGIC, &[0, 2, 1, 2]).unwrap();
        write_u32_file(&paths.fine().postings(), POSTINGS_MAGIC, &[10, 20]).unwrap();
        assert!(H3SpatialIndex::open(&paths).is_err());
    }

    #[test]
    fn open_rejects_offsets_past_postings() {
        let (_dir, paths) = build_sample_index();
        write_u64_file(&paths.fine().offsets(), OFFSETS_MAGIC, &[0, 99]).unwrap();
        assert!(H3SpatialIndex::open(&paths).is_err());
    }

    #[test]
    fn build_rejects_place_id_above_u32_max() {
        let dir = tempfile::tempdir().unwrap();
        let paths = SpatialPaths::new(dir.path().join("spatial"));
        let places = vec![place(u64::from(u32::MAX) + 1, 48.8566, 2.3522, 0.9)];
        let err = build_index(&places, &paths).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("exceeds u32") || msg.contains("32-bit"),
            "unexpected error: {msg}"
        );
    }
}
