#![expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]
use hexplace_core::{AddressParts, OsmType};

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
fn reverse_finds_nearby_place() -> TestResult {
    let dir = tempfile::tempdir()?;
    let paths = SpatialPaths::new(dir.path().join("spatial"));
    let places = vec![place(0, 48.8566, 2.3522, 0.9)];
    build_index(&places, &paths)?;
    let index = H3SpatialIndex::open(&paths)?;
    let q = ReverseQuery::new(48.8566, 2.3522, Some(5))?;
    let ids = index.candidates(&q)?;
    assert!(
        ids.contains(&0),
        "nearby reverse query must include the seeded place id"
    );
    Ok(())
}

#[test]
fn dense_cell_returns_candidates() -> TestResult {
    let dir = tempfile::tempdir()?;
    let paths = SpatialPaths::new(dir.path().join("spatial"));
    let mut places = Vec::new();
    for i in 0..40u64 {
        // Cluster tightly so many share a fine cell.
        #[expect(
            clippy::cast_precision_loss,
            reason = "test jitter uses small integer offsets only"
        )]
        let lat = (i as f64).mul_add(0.00001, 48.8566);
        places.push(place(i, lat, 2.3522, 0.5));
    }
    build_index(&places, &paths)?;
    let index = H3SpatialIndex::open(&paths)?;
    let q = ReverseQuery::new(48.8566, 2.3522, Some(5))?;
    let ids = index.candidates(&q)?;
    assert!(
        ids.len() >= 5,
        "dense cell must yield at least the requested candidate limit"
    );
    Ok(())
}

#[test]
fn empty_fine_falls_back_to_coarse() -> TestResult {
    let dir = tempfile::tempdir()?;
    let paths = SpatialPaths::new(dir.path().join("spatial"));
    // Place in Paris — present in both fine and coarse after build.
    let places = vec![place(0, 48.8566, 2.3522, 0.9)];
    build_index(&places, &paths)?;

    // Empty the fine layer so candidates must come from coarse.
    let fine = paths.fine();
    write_u64_file(&fine.cells(), CELLS_MAGIC, &[])?;
    write_u64_file(&fine.offsets(), OFFSETS_MAGIC, &[0])?;
    write_u32_file(&fine.postings(), POSTINGS_MAGIC, &[])?;

    let index = H3SpatialIndex::open(&paths)?;
    let q = ReverseQuery::new(48.8566, 2.3522, Some(1))?;
    let ids = index.candidates(&q)?;
    assert!(
        ids.contains(&0),
        "coarse CSR must supply the seeded place when the fine layer is empty"
    );
    Ok(())
}

fn build_sample_index() -> Result<(tempfile::TempDir, SpatialPaths), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let paths = SpatialPaths::new(dir.path().join("spatial"));
    let places = vec![place(0, 48.8566, 2.3522, 0.9)];
    build_index(&places, &paths)?;
    Ok((dir, paths))
}

fn truncate_path(path: &Path, len: u64) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::options().write(true).open(path)?;
    file.set_len(len)?;
    Ok(())
}

#[test]
fn open_rejects_truncated_cells() -> TestResult {
    let (_dir, paths) = build_sample_index()?;
    let cells = paths.fine().cells();
    let len = std::fs::metadata(&cells)?.len();
    truncate_path(&cells, len - 1)?;
    assert!(
        H3SpatialIndex::open(&paths).is_err(),
        "truncated cells column must fail open"
    );
    Ok(())
}

#[test]
fn open_rejects_truncated_postings() -> TestResult {
    let (_dir, paths) = build_sample_index()?;
    let postings = paths.fine().postings();
    let len = std::fs::metadata(&postings)?.len();
    truncate_path(&postings, len - 1)?;
    assert!(
        H3SpatialIndex::open(&paths).is_err(),
        "truncated postings column must fail open"
    );
    Ok(())
}

#[test]
fn open_rejects_non_monotone_offsets() -> TestResult {
    let (_dir, paths) = build_sample_index()?;
    write_u64_file(&paths.fine().cells(), CELLS_MAGIC, &[1, 2, 3])?;
    write_u64_file(&paths.fine().offsets(), OFFSETS_MAGIC, &[0, 2, 1, 2])?;
    write_u32_file(&paths.fine().postings(), POSTINGS_MAGIC, &[10, 20])?;
    assert!(
        H3SpatialIndex::open(&paths).is_err(),
        "non-monotone offsets must fail open"
    );
    Ok(())
}

#[test]
fn open_rejects_offsets_past_postings() -> TestResult {
    let (_dir, paths) = build_sample_index()?;
    write_u64_file(&paths.fine().offsets(), OFFSETS_MAGIC, &[0, 99])?;
    assert!(
        H3SpatialIndex::open(&paths).is_err(),
        "offsets past postings length must fail open"
    );
    Ok(())
}

#[test]
fn build_rejects_place_id_above_u32_max() -> TestResult {
    let dir = tempfile::tempdir()?;
    let paths = SpatialPaths::new(dir.path().join("spatial"));
    let places = vec![place(u64::from(u32::MAX) + 1, 48.8566, 2.3522, 0.9)];
    let Err(err) = build_index(&places, &paths) else {
        return Err("place_id above u32::MAX must fail spatial build".into());
    };
    let msg = err.to_string();
    assert!(
        msg.contains("exceeds u32") || msg.contains("32-bit"),
        "oversized place_id must fail with a 32-bit range message"
    );
    Ok(())
}
