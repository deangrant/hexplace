//! Concrete storage, indexes, import, and geocoding for Hexplace.

// `memmap2` requires `unsafe` at the mapping boundary; all other code stays safe.

pub mod bench;
mod binio;
pub mod display;
pub mod import;
pub mod manifest;
pub mod ranking;
pub mod service;
pub mod spatial;
pub mod store;
pub mod text;
pub mod tokenize;

#[doc(inline)]
pub use bench::{bench_reverse, BenchReport};
#[doc(inline)]
pub use import::node_store::{FlatNodeStore, NodeStore, SparseNodeStore};
#[doc(inline)]
pub use import::{import_pbf, import_places};
#[doc(inline)]
pub use manifest::{DataPaths, Manifest, SCHEMA_VERSION};
#[doc(inline)]
pub use service::{Engine, EngineConfig};
#[doc(inline)]
pub use store::{MmapPlaceStore, PlacePaths, PlaceStoreWriter};
