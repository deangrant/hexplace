//! Integration tests for import and geocoding.

use hexplace_core::{
    AddressParts, BatchItem, BatchRequest, Geocoder, OsmType, Place, ReverseQuery, SearchQuery,
};
use hexplace_engine::import::{import_pbf, import_places};
use hexplace_engine::{Engine, EngineConfig};

fn sample_places() -> Vec<Place> {
    vec![
        Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 100,
            lat: 48.8566,
            lon: 2.3522,
            name: Some("Paris".into()),
            display_name: String::new(),
            category: "place".into(),
            type_name: "city".into(),
            address: AddressParts {
                city: Some("Paris".into()),
                country: Some("France".into()),
                country_code: Some("fr".into()),
                ..AddressParts::default()
            },
            importance: 0.9,
        },
        Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 101,
            lat: 48.8606,
            lon: 2.3376,
            name: Some("Louvre Museum".into()),
            display_name: String::new(),
            category: "tourism".into(),
            type_name: "museum".into(),
            address: AddressParts {
                road: Some("Rue de Rivoli".into()),
                city: Some("Paris".into()),
                country: Some("France".into()),
                ..AddressParts::default()
            },
            importance: 0.7,
        },
        Place {
            place_id: 0,
            osm_type: OsmType::Way,
            osm_id: 200,
            lat: 48.8584,
            lon: 2.2945,
            name: Some("Avenue des Champs-Elysees".into()),
            display_name: String::new(),
            category: "highway".into(),
            type_name: "primary".into(),
            address: AddressParts {
                road: Some("Avenue des Champs-Elysees".into()),
                city: Some("Paris".into()),
                ..AddressParts::default()
            },
            importance: 0.5,
        },
    ]
}

#[test]
fn import_and_geocode_known_name() {
    let dir = tempfile::tempdir().unwrap();
    import_places(sample_places(), dir.path()).unwrap();
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();
    let hits = engine
        .geocode(&SearchQuery::new("Louvre Museum", Some(5)).unwrap())
        .unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].display_name.to_lowercase().contains("louvre"));
}

#[test]
fn geocode_treats_and_as_literal_term() {
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
    let dir = tempfile::tempdir().unwrap();
    import_places(places, dir.path()).unwrap();
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();

    for q in ["fish and chips", "Fish AND Chips"] {
        let hits = engine.geocode(&SearchQuery::new(q, Some(5)).unwrap()).unwrap();
        assert!(
            hits.iter().any(|h| h.name.as_deref() == Some("Fish and Chips")),
            "expected literal-term hit for query {q:?}, got {hits:?}"
        );
    }
}

#[test]
fn geocode_importance_rerank_beats_text_top1_cut() {
    // Many same-name low-importance hits would win a BM25-only TopDocs(1)
    // cut; overfetch + importance re-rank should promote the city.
    let mut places = Vec::new();
    for i in 0..20u64 {
        places.push(Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 1000 + i,
            lat: 40.0 + (i as f64) * 0.01,
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

    let dir = tempfile::tempdir().unwrap();
    import_places(places, dir.path()).unwrap();
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();
    let hits = engine
        .geocode(&SearchQuery::new("Springfield", Some(1)).unwrap())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name.as_deref(), Some("Springfield"));
    assert_eq!(hits[0].category, "place");
    assert_eq!(hits[0].type_name, "city");
    assert_eq!(hits[0].osm_id, 42);
}

#[test]
fn import_and_reverse_known_point() {
    let dir = tempfile::tempdir().unwrap();
    import_places(sample_places(), dir.path()).unwrap();
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();
    let hits = engine
        .reverse(&ReverseQuery::new(48.8566, 2.3522, Some(3)).unwrap())
        .unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().any(|h| h.name.as_deref() == Some("Paris")));
}

#[test]
fn batch_mixed_operations() {
    let dir = tempfile::tempdir().unwrap();
    import_places(sample_places(), dir.path()).unwrap();
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();
    let response = engine
        .batch(&BatchRequest {
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
        })
        .unwrap();
    assert_eq!(response.items.len(), 2);
    assert!(response.items[0].error.is_none());
    assert!(!response.items[0].results.is_empty());
    assert!(response.items[1].error.is_none());
    assert!(!response.items[1].results.is_empty());
}

#[test]
fn import_pbf_fixture_when_present() {
    let pbf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/tiny.osm.pbf");
    if !pbf.exists() {
        eprintln!("skipping PBF fixture test; missing {}", pbf.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let manifest = import_pbf(&pbf, dir.path()).unwrap();
    assert!(manifest.place_count > 0);
    let engine = Engine::open(EngineConfig::new(dir.path())).unwrap();
    let hits = engine
        .geocode(&SearchQuery::new("Monaco", Some(5)).unwrap())
        .unwrap();
    assert!(
        !hits.is_empty(),
        "expected Monaco search hits from fixture extract"
    );
}

#[test]
fn reimport_replaces_live_tree_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    import_places(sample_places(), &data).unwrap();
    let first = Engine::open(EngineConfig::new(&data)).unwrap();
    assert_eq!(first.manifest().place_count, 3);
    drop(first);

    let mut fewer = sample_places();
    fewer.truncate(1);
    import_places(fewer, &data).unwrap();
    let second = Engine::open(EngineConfig::new(&data)).unwrap();
    assert_eq!(second.manifest().place_count, 1);

    let parent = data.parent().unwrap();
    let name = data.file_name().unwrap().to_str().unwrap();
    let staging_prefix = format!(".{name}.staging-");
    let obsolete_prefix = format!(".{name}.obsolete-");
    for entry in std::fs::read_dir(parent).unwrap() {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.starts_with(&staging_prefix) && !name.starts_with(&obsolete_prefix),
            "leftover publish sibling: {name}"
        );
    }
}

#[test]
fn reimport_rejected_while_engine_holds_lock() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    import_places(sample_places(), &data).unwrap();
    let engine = Engine::open(EngineConfig::new(&data)).unwrap();
    assert_eq!(engine.manifest().place_count, 3);

    let err = import_places(sample_places(), &data).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("data directory in use"),
        "unexpected error: {msg}"
    );
    assert_eq!(engine.manifest().place_count, 3);
    assert!(data.join("manifest.json").is_file());
}
