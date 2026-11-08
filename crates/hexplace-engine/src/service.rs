//! Geocoding service orchestration.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use hexplace_core::{
    BatchItem, BatchRequest, BatchResponse, BatchResult, CoreError, Geocoder, PlaceHit, PlaceId,
    PlaceStore, ReverseBulkHit, ReverseQuery, SearchQuery, SpatialSearcher, TextSearcher,
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
    #[must_use]
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
    #[expect(dead_code, reason = "lock must be held for Engine lifetime")]
    data_lock: File,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("data_dir", &self.paths.root)
            .field("place_count", &self.manifest.place_count)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Maximum number of items accepted in a single batch request.
    pub const MAX_BATCH_ITEMS: usize = 100_000;

    /// Opens indexes from an imported data directory.
    ///
    /// # Errors
    ///
    /// Returns a storage or I/O error when the lock, manifest, or indexes cannot
    /// be opened, or when `place_count` disagrees with the store length.
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
    #[must_use]
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Returns the data directory root.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.paths.root
    }

    /// Reverse-geocodes many `[lat, lon]` points in parallel.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when `points` is empty or exceeds
    /// [`Self::MAX_BATCH_ITEMS`], or when `cancel` is set during work.
    pub fn reverse_bulk(&self, points: &[[f64; 2]]) -> Result<Vec<ReverseBulkHit>, CoreError> {
        self.reverse_bulk_with_cancel(points, None)
    }

    /// Like [`Self::reverse_bulk`], stopping early when `cancel` becomes true.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when `points` is empty or exceeds
    /// [`Self::MAX_BATCH_ITEMS`], or when cancellation is observed.
    pub fn reverse_bulk_with_cancel(
        &self,
        points: &[[f64; 2]],
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<ReverseBulkHit>, CoreError> {
        if points.is_empty() {
            return Err(CoreError::invalid("points must not be empty"));
        }
        if points.len() > Self::MAX_BATCH_ITEMS {
            return Err(CoreError::invalid(format!(
                "bulk limited to {} points",
                Self::MAX_BATCH_ITEMS
            )));
        }
        check_cancel(cancel)?;

        let workers = worker_count(points.len());
        let chunk_size = points.len().div_ceil(workers);
        let mut slots: Vec<Option<ReverseBulkHit>> = (0..points.len()).map(|_| None).collect();
        let mut cancelled = false;

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for (chunk_idx, chunk) in points.chunks(chunk_size).enumerate() {
                let start = chunk_idx * chunk_size;
                let end = start + chunk.len();
                handles.push((
                    start..end,
                    scope.spawn(move || {
                        let mut local = Vec::with_capacity(chunk.len());
                        for (offset, point) in chunk.iter().enumerate() {
                            if is_cancelled(cancel) {
                                return Err(());
                            }
                            let index = start + offset;
                            let [lat, lon] = *point;
                            local.push((index, reverse_one_bulk_hit(self, index, lat, lon)));
                        }
                        Ok(local)
                    }),
                ));
            }
            cancelled =
                fill_slots_from_cancelable_joins(&mut slots, handles, |idx| ReverseBulkHit {
                    index: idx,
                    place_id: None,
                    score: None,
                    display_name: None,
                    error: Some("bulk worker panicked".into()),
                });
        });

        if cancelled {
            return Err(CoreError::invalid("request cancelled"));
        }

        Ok(slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                slot.unwrap_or_else(|| ReverseBulkHit {
                    index: idx,
                    place_id: None,
                    score: None,
                    display_name: None,
                    error: Some("bulk worker panicked".into()),
                })
            })
            .collect())
    }

    fn process_item(&self, item: &BatchItem) -> BatchResult {
        match item {
            BatchItem::Geocode { id, q, limit } => {
                let result = SearchQuery::new(q, *limit).and_then(|query| self.geocode(&query));
                batch_result_from(id.clone(), result)
            }
            BatchItem::Reverse {
                id,
                lat,
                lon,
                limit,
            } => {
                let result =
                    ReverseQuery::new(*lat, *lon, *limit).and_then(|query| self.reverse(&query));
                batch_result_from(id.clone(), result)
            }
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
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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
        self.batch_with_cancel(request, None)
    }
}

impl Engine {
    /// Like [`Geocoder::batch`], stopping early when `cancel` becomes true.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when the batch is empty or oversized, or
    /// when cancellation is observed.
    pub fn batch_with_cancel(
        &self,
        request: &BatchRequest,
        cancel: Option<&AtomicBool>,
    ) -> Result<BatchResponse, CoreError> {
        if request.items.is_empty() {
            return Err(CoreError::invalid("batch items must not be empty"));
        }
        if request.items.len() > Self::MAX_BATCH_ITEMS {
            return Err(CoreError::invalid(format!(
                "batch limited to {} items",
                Self::MAX_BATCH_ITEMS
            )));
        }
        check_cancel(cancel)?;

        let workers = worker_count(request.items.len());
        let chunk_size = request.items.len().div_ceil(workers);
        let mut slots: Vec<Option<BatchResult>> = (0..request.items.len()).map(|_| None).collect();
        let mut cancelled = false;

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
                            if is_cancelled(cancel) {
                                return Err(());
                            }
                            local.push((start + offset, self.process_item(item)));
                        }
                        Ok(local)
                    }),
                ));
            }
            cancelled = fill_slots_from_cancelable_joins(&mut slots, handles, |idx| {
                request.items.get(idx).map_or_else(
                    || BatchResult {
                        id: None,
                        results: Vec::new(),
                        error: Some("batch worker panicked".into()),
                    },
                    |item| batch_item_error(item, "batch worker panicked"),
                )
            });
        });

        if cancelled {
            return Err(CoreError::invalid("request cancelled"));
        }

        let items = slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                slot.unwrap_or_else(|| {
                    request.items.get(idx).map_or_else(
                        || BatchResult {
                            id: None,
                            results: Vec::new(),
                            error: Some("batch worker panicked".into()),
                        },
                        |item| batch_item_error(item, "batch worker panicked"),
                    )
                })
            })
            .collect();
        Ok(BatchResponse { items })
    }
}

fn worker_count(len: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(len)
        .max(1)
}

type CancelableJoinHandles<'scope, T> = Vec<(
    std::ops::Range<usize>,
    std::thread::ScopedJoinHandle<'scope, Result<Vec<(usize, T)>, ()>>,
)>;

/// Fills `slots` from worker joins. Returns `true` when any worker observed cancel.
fn fill_slots_from_cancelable_joins<T, F>(
    slots: &mut [Option<T>],
    handles: CancelableJoinHandles<'_, T>,
    panic_value: F,
) -> bool
where
    F: Fn(usize) -> T,
{
    let mut cancelled = false;
    for (range, handle) in handles {
        match handle.join() {
            Ok(Ok(local)) => {
                for (idx, value) in local {
                    if let Some(slot) = slots.get_mut(idx) {
                        *slot = Some(value);
                    }
                }
            }
            Ok(Err(())) => {
                cancelled = true;
            }
            Err(_) => {
                for idx in range {
                    if let Some(slot) = slots.get_mut(idx) {
                        *slot = Some(panic_value(idx));
                    }
                }
            }
        }
    }
    cancelled
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), CoreError> {
    if is_cancelled(cancel) {
        return Err(CoreError::invalid("request cancelled"));
    }
    Ok(())
}

fn reverse_one_bulk_hit(engine: &Engine, index: usize, lat: f64, lon: f64) -> ReverseBulkHit {
    let query = match ReverseQuery::new(lat, lon, Some(1)) {
        Ok(query) => query,
        Err(e) => return bulk_hit_error(index, e.to_string()),
    };
    let results = match engine.reverse(&query) {
        Ok(results) => results,
        Err(e) => return bulk_hit_error(index, e.to_string()),
    };
    let Some(top) = results.first() else {
        return bulk_hit_error(index, "no results".into());
    };
    ReverseBulkHit {
        index,
        place_id: Some(top.place_id),
        score: Some(top.score),
        display_name: Some(top.display_name.clone()),
        error: None,
    }
}

const fn bulk_hit_error(index: usize, message: String) -> ReverseBulkHit {
    ReverseBulkHit {
        index,
        place_id: None,
        score: None,
        display_name: None,
        error: Some(message),
    }
}

fn batch_result_from(id: Option<String>, result: Result<Vec<PlaceHit>, CoreError>) -> BatchResult {
    match result {
        Ok(results) => BatchResult {
            id,
            results,
            error: None,
        },
        Err(e) => BatchResult {
            id,
            results: Vec::new(),
            error: Some(e.to_string()),
        },
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
#[path = "service_tests.rs"]
mod tests;
