//! OSM PBF import into places and secondary indexes.

pub mod node_store;
mod publish;

use std::collections::HashMap;
use std::path::Path;

use hexplace_core::{AddressParts, CoreError, OsmType, Place};
use osmpbf::{Element, ElementReader};
use tracing::info;

use crate::display::{ensure_display_name, format_display_name};
use crate::manifest::{hash_file, DataPaths, Manifest};
use crate::spatial;
use crate::store::PlaceStoreWriter;
use crate::text;

use self::node_store::{deg_to_e7, e7_to_deg, NodeStore, SparseNodeStore};
use self::publish::{
    cleanup_orphans, create_staging_dir, discard_staging, publish_data_dir,
};

pub use publish::acquire_shared_lock;

/// Imports an OSM PBF file into a Hexplace data directory.
pub fn import_pbf(pbf_path: &Path, data_dir: &Path) -> Result<Manifest, CoreError> {
    cleanup_orphans(data_dir)?;

    info!(path = %pbf_path.display(), "reading OSM PBF");
    let extracted = extract_places(pbf_path)?;
    info!(count = extracted.len(), "extracted searchable places");

    if extracted.is_empty() {
        return Err(CoreError::import(
            "no searchable places found in the input file",
        ));
    }

    let source_hash = hash_file(pbf_path)?;
    let manifest = Manifest::new(
        pbf_path.display().to_string(),
        source_hash,
        extracted.len() as u64,
    );
    finish_import(extracted, &manifest, data_dir)?;
    info!(places = manifest.place_count, "import complete");
    Ok(manifest)
}

/// Builds a data directory from an in-memory place list (tests and fixtures).
pub fn import_places(places: Vec<Place>, data_dir: &Path) -> Result<Manifest, CoreError> {
    cleanup_orphans(data_dir)?;
    let mut normalized = Vec::with_capacity(places.len());
    for mut place in places {
        ensure_display_name(&mut place);
        normalized.push(place);
    }
    if normalized.is_empty() {
        return Err(CoreError::import("no places to import"));
    }
    let manifest = Manifest::new("memory://places", "0", normalized.len() as u64);
    finish_import(normalized, &manifest, data_dir)?;
    Ok(manifest)
}

fn finish_import(
    places: Vec<Place>,
    manifest: &Manifest,
    data_dir: &Path,
) -> Result<(), CoreError> {
    let staging = create_staging_dir(data_dir)?;
    let paths = DataPaths::new(&staging);
    if let Err(e) = (|| {
        paths.ensure_layout()?;
        write_indexes(places, &paths)?;
        manifest.save(&paths.manifest())?;
        publish_data_dir(&staging, data_dir)
    })() {
        discard_staging(&staging);
        return Err(e);
    }
    Ok(())
}

fn write_indexes(places: Vec<Place>, paths: &DataPaths) -> Result<(), CoreError> {
    let mut writer = PlaceStoreWriter::new();
    for place in places {
        writer.push(place);
    }
    writer.write_to(&paths.places())?;
    text::build_index(writer.places(), &paths.text_dir())?;
    spatial::build_index(writer.places(), &paths.spatial())?;
    Ok(())
}

fn extract_places(pbf_path: &Path) -> Result<Vec<Place>, CoreError> {
    let mut coords = SparseNodeStore::new();
    let reader = ElementReader::from_path(pbf_path)
        .map_err(|e| CoreError::import(format!("failed to open PBF: {e}")))?;
    reader
        .for_each(|element| {
            if let Element::Node(node) = element {
                let _ = coords.insert(node.id(), deg_to_e7(node.lat()), deg_to_e7(node.lon()));
            } else if let Element::DenseNode(node) = element {
                let _ = coords.insert(node.id(), deg_to_e7(node.lat()), deg_to_e7(node.lon()));
            }
        })
        .map_err(|e| CoreError::import(format!("PBF read failed: {e}")))?;
    coords.finalize()?;

    let mut places = Vec::new();
    let reader = ElementReader::from_path(pbf_path)
        .map_err(|e| CoreError::import(format!("failed to reopen PBF: {e}")))?;
    reader
        .for_each(|element| match element {
            Element::Node(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect();
                if let Some(mut place) = place_from_tags(
                    OsmType::Node,
                    node.id() as u64,
                    node.lat(),
                    node.lon(),
                    &tags,
                ) {
                    ensure_display_name(&mut place);
                    places.push(place);
                }
            }
            Element::DenseNode(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect();
                if let Some(mut place) = place_from_tags(
                    OsmType::Node,
                    node.id() as u64,
                    node.lat(),
                    node.lon(),
                    &tags,
                ) {
                    ensure_display_name(&mut place);
                    places.push(place);
                }
            }
            Element::Way(way) => {
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect();
                if !is_named_highway(&tags) && !has_searchable_name(&tags) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                let Some((lat, lon)) = centroid(&refs, &coords) else {
                    return;
                };
                if let Some(mut place) =
                    place_from_tags(OsmType::Way, way.id() as u64, lat, lon, &tags)
                {
                    ensure_display_name(&mut place);
                    places.push(place);
                }
            }
            Element::Relation(_) => {
                // Relations (admin boundaries, multipolygon POIs) are not imported.
            }
        })
        .map_err(|e| CoreError::import(format!("PBF read failed: {e}")))?;

    Ok(places)
}

fn tag_map<'a>(tags: &'a [(String, String)]) -> HashMap<&'a str, &'a str> {
    tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

fn has_searchable_name(tags: &[(String, String)]) -> bool {
    let map = tag_map(tags);
    map.contains_key("name") || map.contains_key("name:en")
}

fn is_named_highway(tags: &[(String, String)]) -> bool {
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
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1].is_ascii_alphabetic() {
        Some(value.to_ascii_lowercase())
    } else {
        None
    }
}

fn place_from_tags(
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

    let country_code = map
        .get("ISO3166-1:alpha2")
        .and_then(|s| iso2_code(s))
        .or_else(|| map.get("addr:country").and_then(|s| iso2_code(s)));

    let address = AddressParts {
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
    };

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

fn centroid(refs: &[i64], coords: &SparseNodeStore) -> Option<(f64, f64)> {
    let mut sum_lat = 0.0;
    let mut sum_lon = 0.0;
    let mut n = 0usize;
    for id in refs {
        if let Some((lat_e7, lon_e7)) = coords.get(*id) {
            sum_lat += e7_to_deg(lat_e7);
            sum_lon += e7_to_deg(lon_e7);
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some((sum_lat / n as f64, sum_lon / n as f64))
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
