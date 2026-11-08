//! Capability traits for storage and search (DIP / ISP).

use crate::error::CoreError;
use crate::place::{Place, PlaceHit, PlaceId};
use crate::query::{BatchRequest, BatchResponse, ReverseQuery, SearchQuery};

/// Reads place records by dense identifier.
pub trait PlaceStore {
    /// Returns one place by id.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the id is absent, or a storage
    /// variant when the backing store cannot be read.
    fn get(&self, id: PlaceId) -> Result<Place, CoreError>;

    /// Returns WGS84 coordinates without hydrating string fields.
    ///
    /// # Errors
    ///
    /// Propagates failures from [`Self::get`].
    fn coord(&self, id: PlaceId) -> Result<(f64, f64), CoreError> {
        let place = self.get(id)?;
        Ok((place.lat, place.lon))
    }

    /// Returns importance without hydrating string fields.
    ///
    /// # Errors
    ///
    /// Propagates failures from [`Self::get`].
    fn importance(&self, id: PlaceId) -> Result<f32, CoreError> {
        Ok(self.get(id)?.importance)
    }

    /// Returns many places; missing ids become errors.
    ///
    /// # Errors
    ///
    /// Propagates the first failure from [`Self::get`] for any id.
    fn places(&self, ids: &[PlaceId]) -> Result<Vec<Place>, CoreError> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

/// Forward text search over indexed places.
pub trait TextSearcher {
    /// Returns ranked `(place_id, score)` pairs for a free-text query.
    ///
    /// # Errors
    ///
    /// Returns an index or storage [`CoreError`] when search cannot run.
    fn search(&self, query: &SearchQuery) -> Result<Vec<(PlaceId, f32)>, CoreError>;
}

/// Reverse spatial candidate lookup.
pub trait SpatialSearcher {
    /// Returns candidate place ids near a coordinate, nearest-first preferred.
    ///
    /// # Errors
    ///
    /// Returns an index or storage [`CoreError`] when lookup cannot run.
    fn candidates(&self, query: &ReverseQuery) -> Result<Vec<PlaceId>, CoreError>;
}

/// High-level geocoding policy over store and indexes.
pub trait Geocoder {
    /// Forward geocode by free-text query.
    ///
    /// # Errors
    ///
    /// Returns a [`CoreError`] when search or place hydration fails.
    fn geocode(&self, query: &SearchQuery) -> Result<Vec<PlaceHit>, CoreError>;

    /// Reverse geocode by coordinate.
    ///
    /// # Errors
    ///
    /// Returns a [`CoreError`] when candidate lookup or hydration fails.
    fn reverse(&self, query: &ReverseQuery) -> Result<Vec<PlaceHit>, CoreError>;

    /// Process a bulk request of mixed operations.
    ///
    /// # Errors
    ///
    /// Returns a [`CoreError`] when the batch cannot be processed as a whole.
    /// Per-item failures may instead appear on [`BatchResponse`] items.
    fn batch(&self, request: &BatchRequest) -> Result<BatchResponse, CoreError>;
}
