//! Geocoding service orchestration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use geofind_core::{
    BatchItem, BatchRequest, BatchResponse, BatchResult, CoreError, Geocoder, PlaceHit, PlaceId,
    PlaceStore, ReverseQuery, SearchQuery, SpatialSearcher, TextSearcher,
};

use crate::manifest::{DataPaths, Manifest};
use crate::ranking::{forward_score, haversine_m, reverse_score};
use crate::shard::{ShardRouter, SingleShard};
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
    #[allow(dead_code)]
    shard_router: Arc<dyn ShardRouter>,
}

impl Engine {
    /// Maximum number of items accepted in a single batch request.
    pub const MAX_BATCH_ITEMS: usize = 100_000;

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
            shard_router: Arc::new(SingleShard),
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

    fn process_item(&self, item: &BatchItem) -> BatchResult {
        match item {
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
        }
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
        let mut best: Vec<(f32, PlaceId)> = Vec::with_capacity(query.limit);
        for id in candidates {
            let (lat, lon) = self.store.coord(id)?;
            let importance = self.store.importance(id)?;
            let distance = haversine_m(query.point.lat, query.point.lon, lat, lon);
            let score = reverse_score(distance, importance);
            push_best(&mut best, score, id, query.limit);
        }
        let mut results = Vec::with_capacity(best.len());
        for (score, id) in best {
            let place = self.store.get(id)?;
            results.push(PlaceHit::from_place(&place, score));
        }
        Ok(results)
    }

    fn batch(&self, request: &BatchRequest) -> Result<BatchResponse, CoreError> {
        if request.items.is_empty() {
            return Err(CoreError::invalid("batch items must not be empty"));
        }
        if request.items.len() > Self::MAX_BATCH_ITEMS {
            return Err(CoreError::invalid(format!(
                "batch limited to {} items",
                Self::MAX_BATCH_ITEMS
            )));
        }

        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(request.items.len())
            .max(1);
        let chunk_size = (request.items.len() + workers - 1) / workers;
        let mut slots: Vec<Option<BatchResult>> = (0..request.items.len()).map(|_| None).collect();

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for (chunk_idx, chunk) in request.items.chunks(chunk_size).enumerate() {
                let start = chunk_idx * chunk_size;
                handles.push(scope.spawn(move || {
                    let mut local = Vec::with_capacity(chunk.len());
                    for (offset, item) in chunk.iter().enumerate() {
                        local.push((start + offset, self.process_item(item)));
                    }
                    local
                }));
            }
            for handle in handles {
                if let Ok(local) = handle.join() {
                    for (idx, result) in local {
                        slots[idx] = Some(result);
                    }
                }
            }
        });

        let items = slots
            .into_iter()
            .map(|slot| slot.expect("batch slot filled"))
            .collect();
        Ok(BatchResponse { items })
    }
}

fn push_best(best: &mut Vec<(f32, PlaceId)>, score: f32, id: PlaceId, limit: usize) {
    if best.len() < limit {
        best.push((score, id));
        best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        return;
    }
    if let Some((worst, _)) = best.last() {
        if score > *worst {
            best.pop();
            best.push((score, id));
            best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        }
    }
}
