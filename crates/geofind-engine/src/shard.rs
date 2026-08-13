//! Optional multi-shard routing seam for future scale-out.

use h3o::{LatLng, Resolution};

use crate::spatial::H3_RESOLUTION_COARSE;

/// Selects a shard index for a geographic point.
pub trait ShardRouter: Send + Sync {
    /// Returns the shard index for `(lat, lon)`.
    fn shard_for_point(&self, lat: f64, lon: f64) -> usize;

    /// Number of shards this router knows about.
    fn shard_count(&self) -> usize;
}

/// Single-shard router used by the default engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct SingleShard;

impl ShardRouter for SingleShard {
    fn shard_for_point(&self, _lat: f64, _lon: f64) -> usize {
        0
    }

    fn shard_count(&self) -> usize {
        1
    }
}

/// Routes by coarse H3 cell hashed into `shard_count` buckets.
///
/// Scaffolding only: no multi-shard data is produced in this release.
#[derive(Debug, Clone)]
pub struct CoarseH3ShardRouter {
    shard_count: usize,
    coarse: Resolution,
}

impl CoarseH3ShardRouter {
    /// Creates a router with at least one shard.
    pub fn new(shard_count: usize) -> Self {
        let coarse = Resolution::try_from(H3_RESOLUTION_COARSE).expect("valid coarse res");
        Self {
            shard_count: shard_count.max(1),
            coarse,
        }
    }
}

impl ShardRouter for CoarseH3ShardRouter {
    fn shard_for_point(&self, lat: f64, lon: f64) -> usize {
        let Ok(ll) = LatLng::new(lat, lon) else {
            return 0;
        };
        let cell = u64::from(ll.to_cell(self.coarse));
        (cell as usize) % self.shard_count
    }

    fn shard_count(&self) -> usize {
        self.shard_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_shard_always_zero() {
        let r = SingleShard;
        assert_eq!(r.shard_for_point(1.0, 2.0), 0);
        assert_eq!(r.shard_count(), 1);
    }

    #[test]
    fn coarse_router_is_deterministic() {
        let r = CoarseH3ShardRouter::new(4);
        let a = r.shard_for_point(48.85, 2.35);
        let b = r.shard_for_point(48.85, 2.35);
        assert_eq!(a, b);
        assert!(a < 4);
    }
}
