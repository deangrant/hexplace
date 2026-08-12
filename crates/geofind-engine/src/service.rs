//! Geocoding service orchestration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use geofind_core::{
    BatchItem, BatchRequest, BatchResponse, BatchResult, CoreError, Geocoder, PlaceHit, PlaceStore,
    ReverseQuery, SearchQuery, SpatialSearcher, TextSearcher,
};

use crate::manifest::{DataPaths, Manifest};
use crate::ranking::{forward_score, haversine_m, reverse_score};
use crate::spatial::H3SpatialIndex;
use crate::store::MmapPlaceStore;
use crate::text::TantivySearcher;

/// Configuration for opening an engine against a data directory.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the Geofind data directory.
    pub data_dir: PathBuf,
}

impl EngineConfig {
    /// Creates a config for `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

/// Production geocoder over mmap places, Tantivy, and H3 indexes.
pub struct Engine {
    paths: DataPaths,
    manifest: Manifest,
    store: Arc<MmapPlaceStore>,
    text: Arc<TantivySearcher>,
    spatial: Arc<H3SpatialIndex>,
}

impl Engine {
    /// Opens indexes from an imported data directory.
    pub fn open(config: EngineConfig) -> Result<Self, CoreError> {
        let paths = DataPaths::new(config.data_dir);
        let manifest = Manifest::load(&paths.manifest())?;
        let store = Arc::new(MmapPlaceStore::open(&paths.places())?);
        let text = Arc::new(TantivySearcher::open(&paths.text_dir())?);
        let spatial = Arc::new(H3SpatialIndex::open(&paths.spatial())?);
        Ok(Self {
            paths,
            manifest,
            store,
            text,
            spatial,
        })
    }

    /// Returns the loaded manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Returns the data directory root.
    pub fn data_dir(&self) -> &Path {
        &self.paths.root
    }
}

impl Geocoder for Engine {
    fn geocode(&self, query: &SearchQuery) -> Result<Vec<PlaceHit>, CoreError> {
        let hits = self.text.search(query)?;
        let mut results = Vec::with_capacity(hits.len());
        for (id, score) in hits {
            let place = self.store.get(id)?;
            let combined = forward_score(score, &place);
            results.push(PlaceHit::from_place(&place, combined));
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.limit);
        Ok(results)
    }

    fn reverse(&self, query: &ReverseQuery) -> Result<Vec<PlaceHit>, CoreError> {
        let candidates = self.spatial.candidates(query)?;
        let mut results = Vec::new();
        for id in candidates {
            let place = self.store.get(id)?;
            let distance = haversine_m(query.point.lat, query.point.lon, place.lat, place.lon);
            let score = reverse_score(distance, &place);
            results.push(PlaceHit::from_place(&place, score));
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.limit);
        Ok(results)
    }

    fn batch(&self, request: &BatchRequest) -> Result<BatchResponse, CoreError> {
        if request.items.is_empty() {
            return Err(CoreError::invalid("batch items must not be empty"));
        }
        if request.items.len() > 10_000 {
            return Err(CoreError::invalid("batch limited to 10000 items"));
        }
        let mut items = Vec::with_capacity(request.items.len());
        for item in &request.items {
            let result = match item {
                BatchItem::Geocode { id, q, limit } => match SearchQuery::new(q, *limit) {
                    Ok(query) => match self.geocode(&query) {
                        Ok(results) => BatchResult {
                            id: id.clone(),
                            results,
                            error: None,
                        },
                        Err(e) => BatchResult {
                            id: id.clone(),
                            results: Vec::new(),
                            error: Some(e.to_string()),
                        },
                    },
                    Err(e) => BatchResult {
                        id: id.clone(),
                        results: Vec::new(),
                        error: Some(e.to_string()),
                    },
                },
                BatchItem::Reverse {
                    id,
                    lat,
                    lon,
                    limit,
                } => match ReverseQuery::new(*lat, *lon, *limit) {
                    Ok(query) => match self.reverse(&query) {
                        Ok(results) => BatchResult {
                            id: id.clone(),
                            results,
                            error: None,
                        },
                        Err(e) => BatchResult {
                            id: id.clone(),
                            results: Vec::new(),
                            error: Some(e.to_string()),
                        },
                    },
                    Err(e) => BatchResult {
                        id: id.clone(),
                        results: Vec::new(),
                        error: Some(e.to_string()),
                    },
                },
            };
            items.push(result);
        }
        Ok(BatchResponse { items })
    }
}
