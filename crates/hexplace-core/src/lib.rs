//! Domain types and narrow traits for Hexplace geocoding.
//!
//! High-level policy depends on these abstractions; concrete indexes and
//! stores live in the engine crate and are wired at the composition root.

#![forbid(unsafe_code)]

pub mod error;
pub mod place;
pub mod query;
pub mod traits;

#[doc(inline)]
pub use error::CoreError;
#[doc(inline)]
pub use place::{AddressParts, OsmType, Place, PlaceHit, PlaceId};
#[doc(inline)]
pub use query::{
    BatchItem, BatchRequest, BatchResponse, BatchResult, GeoPoint, ReverseBulkHit,
    ReverseBulkRequest, ReverseQuery, SearchQuery,
};
#[doc(inline)]
pub use traits::{Geocoder, PlaceStore, SpatialSearcher, TextSearcher};
