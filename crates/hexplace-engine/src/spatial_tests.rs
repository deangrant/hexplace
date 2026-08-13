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
