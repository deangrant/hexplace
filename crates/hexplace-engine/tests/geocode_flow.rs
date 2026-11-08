//! Forward/reverse geocode and batch integration tests.

#![expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]

mod common;

use common::{sample_places, TestResult};
use hexplace_core::{
    AddressParts, BatchItem, BatchRequest, Geocoder, OsmType, Place, ReverseQuery, SearchQuery,
};
use hexplace_engine::import::{import_pbf, import_places};
use hexplace_engine::{Engine, EngineConfig};

#[test]
fn import_and_geocode_known_name() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let hits = engine.geocode(&SearchQuery::new("Louvre Museum", Some(5))?)?;
    assert!(
        !hits.is_empty(),
        "known museum name must return at least one hit"
    );
    assert!(
        hits[0].display_name.to_lowercase().contains("louvre"),
        "top hit display name must mention the queried museum"
    );
    Ok(())
}

#[test]
fn geocode_treats_and_as_literal_term() -> TestResult {
    let places = vec![Place {
        place_id: 0,
        osm_type: OsmType::Node,
        osm_id: 7,
        lat: 51.5,
        lon: -0.1,
        name: Some("Fish and Chips".into()),
        display_name: String::new(),
        category: "amenity".into(),
        type_name: "fast_food".into(),
        address: AddressParts::default(),
        importance: 0.4,
    }];
    let dir = tempfile::tempdir()?;
    import_places(places, dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;

    for q in ["fish and chips", "Fish AND Chips"] {
        let hits = engine.geocode(&SearchQuery::new(q, Some(5))?)?;
        assert!(
            hits.iter().any(|h| h.name.as_deref() == Some("Fish and Chips")),
            "AND must be treated as a literal token in the place name"
        );
    }
    Ok(())
}

#[test]
fn geocode_treats_or_and_not_as_literal_terms() -> TestResult {
    let places = vec![
        Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 8,
            lat: 51.5,
            lon: -0.1,
            name: Some("Black or White".into()),
            display_name: String::new(),
            category: "amenity".into(),
            type_name: "cafe".into(),
            address: AddressParts::default(),
            importance: 0.4,
        },
        Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 9,
            lat: 51.51,
            lon: -0.11,
            name: Some("Do not Enter".into()),
            display_name: String::new(),
            category: "amenity".into(),
            type_name: "cafe".into(),
            address: AddressParts::default(),
            importance: 0.4,
        },
    ];
    let dir = tempfile::tempdir()?;
    import_places(places, dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;

    let or_hits = engine.geocode(&SearchQuery::new("black or white", Some(5))?)?;
    assert!(
        or_hits.iter().any(|h| h.name.as_deref() == Some("Black or White")),
        "OR must be treated as a literal token in the place name"
    );

    let not_hits = engine.geocode(&SearchQuery::new("do not enter", Some(5))?)?;
    assert!(
        not_hits.iter().any(|h| h.name.as_deref() == Some("Do not Enter")),
        "NOT must be treated as a literal token in the place name"
    );
    Ok(())
}

#[test]
fn geocode_importance_rerank_beats_text_top1_cut() -> TestResult {
    // Many same-name low-importance hits would win a BM25-only TopDocs(1)
    // cut; overfetch + importance re-rank should promote the city.
    let mut places = Vec::new();
    for i in 0..20u64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "test fixtures use small integer latitude offsets"
        )]
        let lat = (i as f64).mul_add(0.01, 40.0);
        places.push(Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 1000 + i,
            lat,
            lon: -90.0,
            name: Some("Springfield".into()),
            display_name: String::new(),
            category: "amenity".into(),
            type_name: "cafe".into(),
            address: AddressParts::default(),
            importance: 0.1,
        });
    }
    places.push(Place {
        place_id: 0,
        osm_type: OsmType::Node,
        osm_id: 42,
        lat: 39.7817,
        lon: -89.6501,
        name: Some("Springfield".into()),
        display_name: String::new(),
        category: "place".into(),
        type_name: "city".into(),
        address: AddressParts {
            city: Some("Springfield".into()),
            ..AddressParts::default()
        },
        importance: 0.9,
    });

    let dir = tempfile::tempdir()?;
    import_places(places, dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let hits = engine.geocode(&SearchQuery::new("Springfield", Some(1))?)?;
    assert_eq!(hits.len(), 1, "limit 1 must return a single hit");
    assert_eq!(
        hits[0].name.as_deref(),
        Some("Springfield"),
        "top hit must keep the Springfield name"
    );
    assert_eq!(
        hits[0].category, "place",
        "importance re-rank must promote the city over cafes"
    );
    assert_eq!(
        hits[0].type_name, "city",
        "importance re-rank must promote the city type"
    );
    assert_eq!(
        hits[0].osm_id, 42,
        "importance re-rank must select the high-importance city osm id"
    );
    Ok(())
}

#[test]
fn import_and_reverse_known_point() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let hits = engine.reverse(&ReverseQuery::new(48.8566, 2.3522, Some(3))?)?;
    assert!(
        !hits.is_empty(),
        "reverse near Paris must return candidates"
    );
    assert!(
        hits.iter().any(|h| h.name.as_deref() == Some("Paris")),
        "reverse near Paris must include the Paris place"
    );
    Ok(())
}

#[test]
fn reverse_skips_out_of_range_place_ids() -> TestResult {
    let dir = tempfile::tempdir()?;
    let Some(first) = sample_places().into_iter().next() else {
        return Err("sample_places must yield at least one place".into());
    };
    import_places(vec![first], dir.path())?;

    // Keep fine cells; expand the single cell's postings with a bogus id.
    let fine = dir.path().join("spatial/fine");
    write_u64_column(&fine.join("cell_offsets.bin"), *b"GFOF", &[0, 2])?;
    write_u32_column(&fine.join("postings.bin"), *b"GFPO", &[0, 999_999])?;

    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let hits = engine.reverse(&ReverseQuery::new(48.8566, 2.3522, Some(3))?)?;
    assert!(
        hits.iter().any(|h| h.name.as_deref() == Some("Paris")),
        "reverse must skip out-of-range posting ids and still return the seeded place"
    );
    Ok(())
}

fn write_u64_column(path: &std::path::Path, magic: [u8; 4], values: &[u64]) -> TestResult {
    use std::io::Write;
    let mut out = std::fs::File::create(path)?;
    out.write_all(&magic)?;
    out.write_all(&2u32.to_le_bytes())?;
    let len = u64::try_from(values.len())?;
    out.write_all(&len.to_le_bytes())?;
    for v in values {
        out.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

fn write_u32_column(path: &std::path::Path, magic: [u8; 4], values: &[u32]) -> TestResult {
    use std::io::Write;
    let mut out = std::fs::File::create(path)?;
    out.write_all(&magic)?;
    out.write_all(&2u32.to_le_bytes())?;
    let len = u64::try_from(values.len())?;
    out.write_all(&len.to_le_bytes())?;
    for v in values {
        out.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

#[test]
fn batch_mixed_operations() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let response = engine.batch(&BatchRequest {
        items: vec![
            BatchItem::Geocode {
                id: Some("a".into()),
                q: "Paris".into(),
                limit: Some(2),
            },
            BatchItem::Reverse {
                id: Some("b".into()),
                lat: 48.8606,
                lon: 2.3376,
                limit: Some(1),
            },
        ],
    })?;
    assert_eq!(
        response.items.len(),
        2,
        "batch must return one result slot per request item"
    );
    assert!(
        response.items[0].error.is_none(),
        "geocode batch item must succeed"
    );
    assert!(
        !response.items[0].results.is_empty(),
        "geocode batch item must return hits"
    );
    assert!(
        response.items[1].error.is_none(),
        "reverse batch item must succeed"
    );
    assert!(
        !response.items[1].results.is_empty(),
        "reverse batch item must return hits"
    );
    Ok(())
}

#[test]
fn batch_rejects_empty_items() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let Err(err) = engine.batch(&BatchRequest { items: vec![] }) else {
        return Err("empty batch must be rejected".into());
    };
    assert!(
        err.to_string().contains("empty"),
        "empty batch must map to the invalid-input error class"
    );
    Ok(())
}

#[test]
fn batch_rejects_over_max_items() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let items = (0..=Engine::MAX_BATCH_ITEMS)
        .map(|i| BatchItem::Geocode {
            id: Some(i.to_string()),
            q: "x".into(),
            limit: Some(1),
        })
        .collect();
    let Err(err) = engine.batch(&BatchRequest { items }) else {
        return Err("oversized batch must be rejected".into());
    };
    assert!(
        err.to_string().contains("batch limited"),
        "oversized batch must map to the batch-limit invalid-input class"
    );
    Ok(())
}

#[test]
fn import_pbf_fixture_finds_monaco() -> TestResult {
    let pbf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/tiny.osm.pbf");
    assert!(
        pbf.exists(),
        "missing required fixture {}; add testdata/tiny.osm.pbf",
        pbf.display()
    );
    let dir = tempfile::tempdir()?;
    let manifest = import_pbf(&pbf, dir.path())?;
    assert!(
        manifest.place_count > 0,
        "fixture PBF must yield searchable places"
    );
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    let hits = engine.geocode(&SearchQuery::new("Monaco", Some(5))?)?;
    assert!(
        !hits.is_empty(),
        "fixture PBF import must make Monaco findable by forward search"
    );
    Ok(())
}
