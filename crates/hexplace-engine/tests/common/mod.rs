//! Shared fixtures for hexplace-engine integration tests.

use hexplace_core::{AddressParts, OsmType, Place};

/// Fallible test return type without panicking helpers.
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Sample Paris-area places used across geocode integration tests.
pub fn sample_places() -> Vec<Place> {
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
