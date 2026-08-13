//! OSM PBF import into places and secondary indexes.

pub mod node_store;
mod osm_place;
mod publish;

use std::path::Path;

use hexplace_core::{CoreError, OsmType, Place};
use osmpbf::{Element, ElementReader};
use tracing::info;

use crate::display::ensure_display_name;
use crate::manifest::{hash_file, DataPaths, Manifest};
use crate::spatial;
use crate::store::PlaceStoreWriter;
use crate::text;

use self::node_store::{deg_to_e7, e7_to_deg, NodeStore, SparseNodeStore};
use self::osm_place::{has_searchable_name, is_named_highway, place_from_tags};
use self::publish::{cleanup_orphans, create_staging_dir, discard_staging, publish_data_dir};

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
                let tags = collect_tags(node.tags());
                push_from_node_tags(
                    &mut places,
                    OsmType::Node,
                    node.id() as u64,
                    node.lat(),
                    node.lon(),
                    &tags,
                );
            }
            Element::DenseNode(node) => {
                let tags = collect_tags(node.tags());
                push_from_node_tags(
                    &mut places,
                    OsmType::Node,
                    node.id() as u64,
                    node.lat(),
                    node.lon(),
                    &tags,
                );
            }
            Element::Way(way) => {
                let tags = collect_tags(way.tags());
                if !is_named_highway(&tags) && !has_searchable_name(&tags) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                let Some((lat, lon)) = centroid(&refs, &coords) else {
                    return;
                };
                push_from_node_tags(&mut places, OsmType::Way, way.id() as u64, lat, lon, &tags);
            }
            Element::Relation(_) => {
                // Relations (admin boundaries, multipolygon POIs) are not imported.
            }
        })
        .map_err(|e| CoreError::import(format!("PBF read failed: {e}")))?;

    Ok(places)
}

fn collect_tags<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<(String, String)> {
    tags.map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

fn push_from_node_tags(
    places: &mut Vec<Place>,
    osm_type: OsmType,
    osm_id: u64,
    lat: f64,
    lon: f64,
    tags: &[(String, String)],
) {
    let Some(mut place) = place_from_tags(osm_type, osm_id, lat, lon, tags) else {
        return;
    };
    ensure_display_name(&mut place);
    places.push(place);
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
