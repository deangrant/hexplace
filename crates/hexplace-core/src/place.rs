//! Place records and search hits.

use serde::{Deserialize, Serialize};

/// Dense identifier assigned during import.
pub type PlaceId = u64;

/// OpenStreetMap element kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsmType {
    /// An OSM node.
    Node,
    /// An OSM way.
    Way,
    /// An OSM relation.
    Relation,
}

impl OsmType {
    /// Returns the single-letter OSM type code.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Node => "N",
            Self::Way => "W",
            Self::Relation => "R",
        }
    }

    /// Parses a single-letter OSM type code.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "N" | "n" | "node" => Some(Self::Node),
            "W" | "w" | "way" => Some(Self::Way),
            "R" | "r" | "relation" => Some(Self::Relation),
            _ => None,
        }
    }
}

/// Structured address fields extracted from OSM tags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressParts {
    /// House number when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub house_number: Option<String>,
    /// Street or road name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    /// City, town, or village.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Postal code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    /// Country name when tagged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// ISO 3166-1 alpha-2 country code when tagged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
}

/// A searchable place stored after import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// Dense place identifier.
    pub place_id: PlaceId,
    /// OSM element kind.
    pub osm_type: OsmType,
    /// OSM element identifier.
    pub osm_id: u64,
    /// WGS84 latitude in degrees.
    pub lat: f64,
    /// WGS84 longitude in degrees.
    pub lon: f64,
    /// Primary display name.
    pub name: Option<String>,
    /// Full formatted label for UI and API responses.
    pub display_name: String,
    /// Coarse category derived from OSM tags (for example `place` or `highway`).
    pub category: String,
    /// Finer type within the category (for example `city` or `residential`).
    pub type_name: String,
    /// Structured address parts when available.
    pub address: AddressParts,
    /// Heuristic importance in `[0, 1]`; higher ranks earlier.
    pub importance: f32,
}

/// A ranked search result hydrated from storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceHit {
    /// Dense place identifier.
    pub place_id: PlaceId,
    /// OSM element kind.
    pub osm_type: OsmType,
    /// OSM element identifier.
    pub osm_id: u64,
    /// WGS84 latitude in degrees.
    pub lat: f64,
    /// WGS84 longitude in degrees.
    pub lon: f64,
    /// Primary name when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Full formatted label.
    pub display_name: String,
    /// Coarse category.
    pub category: String,
    /// Finer type name.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Structured address when useful to callers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<AddressParts>,
    /// Ranking score for this hit.
    pub score: f32,
}

impl PlaceHit {
    /// Builds a hit from a stored place and a ranking score.
    pub fn from_place(place: &Place, score: f32) -> Self {
        let address = if place.address == AddressParts::default() {
            None
        } else {
            Some(place.address.clone())
        };
        Self {
            place_id: place.place_id,
            osm_type: place.osm_type,
            osm_id: place.osm_id,
            lat: place.lat,
            lon: place.lon,
            name: place.name.clone(),
            display_name: place.display_name.clone(),
            category: place.category.clone(),
            type_name: place.type_name.clone(),
            address,
            score,
        }
    }
}
