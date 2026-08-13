//! Geocoding service orchestration.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hexplace_core::{
    BatchItem, BatchRequest, BatchResponse, BatchResult, CoreError, Geocoder, PlaceHit, PlaceId,
    PlaceStore, ReverseQuery, SearchQuery, SpatialSearcher, TextSearcher,
};

use crate::import::acquire_shared_lock;
use crate::manifest::{DataPaths, Manifest};
use crate::ranking::{forward_score, haversine_m, reverse_score};
use crate::spatial::H3SpatialIndex;
use crate::store::MmapPlaceStore;
use crate::text::TantivySearcher;

/// Configuration for opening an engine against a data directory.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the Hexplace data directory.
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
    /// Shared flock keeping import from publishing over a live serve.
    #[expect(dead_code)]
    data_lock: File,
}

impl Engine {
    /// Maximum number of items accepted in a single batch request.
    pub const MAX_BATCH_ITEMS: usize = 100_000;

    /// Opens indexes from an imported data directory.
    pub fn open(config: EngineConfig) -> Result<Self, CoreError> {
        let paths = DataPaths::new(config.data_dir);
        let data_lock = acquire_shared_lock(&paths.root)?;
        let manifest = Manifest::load(&paths.manifest())?;
        let store = Arc::new(MmapPlaceStore::open(&paths.places())?);
        if manifest.place_count != store.len() {
            return Err(CoreError::storage(format!(
                "manifest place_count {} does not match store length {}",
                manifest.place_count,
                store.len()
            )));
        }
        let text = Arc::new(TantivySearcher::open(&paths.text_dir())?);
        let spatial = Arc::new(H3SpatialIndex::open(&paths.spatial())?);
        Ok(Self {
            paths,
            manifest,
            store,
            text,
            spatial,
            data_lock,
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
        // Overfetch text hits so importance re-rank can promote places
        // that fall outside BM25 top-`query.limit`.
        let text_query = SearchQuery {
            q: query.q.clone(),
            limit: SearchQuery::MAX_LIMIT,
        };
        let hits = self.text.search(&text_query)?;
        let mut results = Vec::with_capacity(hits.len());
        for (id, score) in hits {
            let Ok(place) = self.store.get(id) else {
                continue;
            };
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
            let Ok((lat, lon)) = self.store.coord(id) else {
                continue;
            };
            let Ok(importance) = self.store.importance(id) else {
                continue;
            };
            let distance = haversine_m(query.point.lat, query.point.lon, lat, lon);
            let score = reverse_score(distance, importance);
            push_best(&mut best, score, id, query.limit);
        }
        let mut results = Vec::with_capacity(best.len());
        for (score, id) in best {
            let Ok(place) = self.store.get(id) else {
                continue;
            };
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
                let end = start + chunk.len();
                handles.push((
                    start..end,
                    scope.spawn(move || {
                        let mut local = Vec::with_capacity(chunk.len());
                        for (offset, item) in chunk.iter().enumerate() {
                            local.push((start + offset, self.process_item(item)));
                        }
                        local
                    }),
                ));
            }
            for (range, handle) in handles {
                match handle.join() {
                    Ok(local) => {
                        for (idx, result) in local {
                            slots[idx] = Some(result);
                        }
                    }
                    Err(_) => {
                        for (idx, result) in
                            batch_worker_error(&request.items, range, "batch worker panicked")
                        {
                            slots[idx] = Some(result);
                        }
                    }
                }
            }
        });

        let items = slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                slot.unwrap_or_else(|| {
                    batch_item_error(&request.items[idx], "batch worker panicked")
                })
            })
            .collect();
        Ok(BatchResponse { items })
    }
}

fn batch_item_id(item: &BatchItem) -> Option<String> {
    match item {
        BatchItem::Geocode { id, .. } | BatchItem::Reverse { id, .. } => id.clone(),
    }
}

fn batch_item_error(item: &BatchItem, message: &str) -> BatchResult {
    BatchResult {
        id: batch_item_id(item),
        results: Vec::new(),
        error: Some(message.to_owned()),
    }
}

/// Builds error results for a panicked worker's index range.
fn batch_worker_error(
    items: &[BatchItem],
    range: std::ops::Range<usize>,
    message: &str,
) -> Vec<(usize, BatchResult)> {
    range
        .map(|idx| (idx, batch_item_error(&items[idx], message)))
        .collect()
}

fn push_best(best: &mut Vec<(f32, PlaceId)>, score: f32, id: PlaceId, limit: usize) {
    if limit == 0 {
        return;
    }
    if best.len() == limit {
        let Some((worst, _)) = best.last() else {
            return;
        };
        if score <= *worst {
            return;
        }
        best.pop();
    }
    let idx = best.partition_point(|&(s, _)| s > score);
    best.insert(idx, (score, id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_worker_error_fills_range_with_item_ids() {
        let items = vec![
            BatchItem::Geocode {
                id: Some("a".into()),
                q: "Paris".into(),
                limit: Some(1),
            },
            BatchItem::Reverse {
                id: Some("b".into()),
                lat: 48.0,
                lon: 2.0,
                limit: Some(1),
            },
            BatchItem::Geocode {
                id: None,
                q: "Louvre".into(),
                limit: None,
            },
        ];
        let filled = batch_worker_error(&items, 1..3, "batch worker panicked");
        assert_eq!(filled.len(), 2);
        assert_eq!(filled[0].0, 1);
        assert_eq!(filled[0].1.id.as_deref(), Some("b"));
        assert_eq!(
            filled[0].1.error.as_deref(),
            Some("batch worker panicked")
        );
        assert!(filled[0].1.results.is_empty());
        assert_eq!(filled[1].0, 2);
        assert_eq!(filled[1].1.id, None);
        assert_eq!(
            filled[1].1.error.as_deref(),
            Some("batch worker panicked")
        );
    }

    #[test]
    fn push_best_keeps_descending_top_n() {
        let mut best = Vec::new();
        push_best(&mut best, 0.2, 2, 3);
        push_best(&mut best, 0.9, 9, 3);
        push_best(&mut best, 0.5, 5, 3);
        push_best(&mut best, 0.1, 1, 3);
        push_best(&mut best, 0.7, 7, 3);
        assert_eq!(best, vec![(0.9, 9), (0.7, 7), (0.5, 5)]);
    }
}
