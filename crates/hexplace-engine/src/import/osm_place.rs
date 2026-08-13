//! OSM tag → Place policy for import (searchability, classify, importance).

use std::collections::HashMap;

use hexplace_core::{AddressParts, OsmType, Place};

use crate::display::format_display_name;

fn tag_map<'a>(tags: &'a [(String, String)]) -> HashMap<&'a str, &'a str> {
    tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

pub(super) fn has_searchable_name(tags: &[(String, String)]) -> bool {
    let map = tag_map(tags);
    map.contains_key("name") || map.contains_key("name:en")
}

pub(super) fn is_named_highway(tags: &[(String, String)]) -> bool {
    let map = tag_map(tags);
    map.contains_key("highway") && (map.contains_key("name") || map.contains_key("name:en"))
}

fn is_address(tags: &[(String, String)]) -> bool {
    let map = tag_map(tags);
    map.contains_key("addr:housenumber") && map.contains_key("addr:street")
}

/// Returns a lowercase ISO 3166-1 alpha-2 code when `value` is two ASCII letters.
fn iso2_code(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    match bytes {
        [a, b] if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => {
            Some(value.to_ascii_lowercase())
        }
        _ => None,
    }
}

fn address_from_tags(map: &HashMap<&str, &str>) -> AddressParts {
    let country_code = map
        .get("ISO3166-1:alpha2")
        .and_then(|s| iso2_code(s))
        .or_else(|| map.get("addr:country").and_then(|s| iso2_code(s)));

    AddressParts {
        house_number: map.get("addr:housenumber").map(|s| (*s).to_owned()),
        road: map
            .get("addr:street")
            .or_else(|| map.get("name").filter(|_| map.contains_key("highway")))
            .map(|s| (*s).to_owned()),
        city: map
            .get("addr:city")
            .or_else(|| map.get("addr:town"))
            .or_else(|| map.get("addr:village"))
            .map(|s| (*s).to_owned()),
        postcode: map.get("addr:postcode").map(|s| (*s).to_owned()),
        country: map.get("addr:country").map(|s| (*s).to_owned()),
        country_code,
    }
}

pub(super) fn place_from_tags(
    osm_type: OsmType,
    osm_id: u64,
    lat: f64,
    lon: f64,
    tags: &[(String, String)],
) -> Option<Place> {
    let map = tag_map(tags);
    let searchable = has_searchable_name(tags) || is_address(tags);
    if !searchable {
        return None;
    }

    let name = map
        .get("name")
        .or_else(|| map.get("name:en"))
        .map(|s| (*s).to_owned());

    let address = address_from_tags(&map);
    let (category, type_name) = classify(&map);
    let importance = importance_for(&category, &type_name, name.is_some());
    let display_name = format_display_name(name.as_deref(), &address);

    Some(Place {
        place_id: 0,
        osm_type,
        osm_id,
        lat,
        lon,
        name,
        display_name,
        category,
        type_name,
        address,
        importance,
    })
}

fn classify(map: &HashMap<&str, &str>) -> (String, String) {
    for key in [
        "place", "highway", "amenity", "shop", "tourism", "leisure", "office", "building",
    ] {
        if let Some(v) = map.get(key) {
            return (key.to_owned(), (*v).to_owned());
        }
    }
    if map.contains_key("addr:housenumber") {
        return ("place".to_owned(), "house".to_owned());
    }
    ("place".to_owned(), "yes".to_owned())
}

/// Heuristic import-time importance from OSM `category` / `type_name`.
///
/// Named features keep the base weight; unnamed ones are scaled down. This is
/// not a learned, population, or multilingual importance model.
fn importance_for(category: &str, type_name: &str, has_name: bool) -> f32 {
    let base = match (category, type_name) {
        ("place", "city") => 0.9,
        ("place", "town") => 0.75,
        ("place", "suburb" | "neighbourhood" | "quarter") => 0.55,
        ("place", "village" | "hamlet") => 0.45,
        ("place", "house") => 0.35,
        ("highway", _) => 0.4,
        ("amenity" | "shop" | "tourism" | "leisure" | "office", _) => 0.3,
        _ => 0.2,
    };
    if has_name {
        base
    } else {
        base * 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn country_code_prefers_iso3166_alpha2() {
        let place = place_from_tags(
            OsmType::Node,
            1,
            0.0,
            0.0,
            &tags(&[
                ("name", "Berlin"),
                ("place", "city"),
                ("addr:country", "Germany"),
                ("ISO3166-1:alpha2", "DE"),
            ]),
        )
        .unwrap();
        assert_eq!(place.address.country.as_deref(), Some("Germany"));
        assert_eq!(place.address.country_code.as_deref(), Some("de"));
    }

    #[test]
    fn country_code_from_two_letter_addr_country() {
        let place = place_from_tags(
            OsmType::Node,
            2,
            0.0,
            0.0,
            &tags(&[
                ("name", "Paris"),
                ("place", "city"),
                ("addr:country", "fr"),
            ]),
        )
        .unwrap();
        assert_eq!(place.address.country.as_deref(), Some("fr"));
        assert_eq!(place.address.country_code.as_deref(), Some("fr"));
    }

    #[test]
    fn named_amenity_is_searchable() {
        let place = place_from_tags(
            OsmType::Node,
            3,
            0.0,
            0.0,
            &tags(&[("name", "Cafe"), ("amenity", "cafe")]),
        );
        assert!(place.is_some());
    }

    #[test]
    fn unnamed_amenity_without_address_is_dropped() {
        let place = place_from_tags(
            OsmType::Node,
            4,
            0.0,
            0.0,
            &tags(&[("amenity", "cafe")]),
        );
        assert!(place.is_none());
    }
}
