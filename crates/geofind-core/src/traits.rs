//! Capability traits for storage and search (DIP / ISP).

use crate::error::CoreError;
use crate::place::{Place, PlaceHit, PlaceId};
use crate::query::{BatchRequest, BatchResponse, ReverseQuery, SearchQuery};

/// Reads place records by dense identifier.
pub trait PlaceStore {
    /// Returns one place by id.
    fn get(&self, id: PlaceId) -> Result<Place, CoreError>;

    /// Returns many places; missing ids become errors.
    fn get_many(&self, ids: &[PlaceId]) -> Result<Vec<Place>, CoreError> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

/// Forward text search over indexed places.
pub trait TextSearcher {
    /// Returns ranked `(place_id, score)` pairs for a free-text query.
    fn search(&self, query: &SearchQuery) -> Result<Vec<(PlaceId, f32)>, CoreError>;
}

/// Reverse spatial candidate lookup.
pub trait SpatialSearcher {
    /// Returns candidate place ids near a coordinate, nearest-first preferred.
    fn candidates(&self, query: &ReverseQuery) -> Result<Vec<PlaceId>, CoreError>;
}

/// High-level geocoding policy over store and indexes.
pub trait Geocoder {
    /// Forward geocode by free-text query.
    fn geocode(&self, query: &SearchQuery) -> Result<Vec<PlaceHit>, CoreError>;

    /// Reverse geocode by coordinate.
    fn reverse(&self, query: &ReverseQuery) -> Result<Vec<PlaceHit>, CoreError>;

    /// Process a bulk request of mixed operations.
    fn batch(&self, request: &BatchRequest) -> Result<BatchResponse, CoreError>;
}
