//! Integration tests for import and geocoding.

use geofind_core::{
    AddressParts, BatchItem, BatchRequest, Geocoder, OsmType, Place, ReverseQuery, SearchQuery,
};
use geofind_engine::import::{import_pbf, import_places};
use geofind_engine::{Engine, EngineConfig};

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
