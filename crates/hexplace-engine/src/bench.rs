//! Reverse bulk timing harness used by `hexplace bench`.

use std::time::Instant;

use hexplace_core::{CoreError, Geocoder, ReverseQuery};

use crate::service::Engine;

/// Summary of a reverse bulk timing run.
#[derive(Debug, Clone)]
pub struct BenchReport {
    /// Number of reverse lookups performed.
    pub count: usize,
    /// Total wall time in seconds.
    pub total_secs: f64,
    /// Median latency in milliseconds.
    pub p50_ms: f64,
    /// 99th percentile latency in milliseconds.
    pub p99_ms: f64,
    /// Effective lookups per second.
    pub qps: f64,
}

/// Times `count` reverse lookups over points near `seed`.
///
/// # Errors
///
/// Returns the first reverse lookup or query-construction failure.
pub fn bench_reverse(
    engine: &Engine,
    seed_lat: f64,
    seed_lon: f64,
    count: usize,
) -> Result<BenchReport, CoreError> {
    let mut latencies_ms = Vec::with_capacity(count);
    let start = Instant::now();
    for i in 0..count {
        // Small deterministic jitter so we do not hit one identical cell only.
        let lat = seed_lat + ((i % 50) as f64) * 0.0001;
        let lon = seed_lon + ((i % 70) as f64) * 0.0001;
        let query = ReverseQuery::new(lat, lon, Some(1))?;
        let item_start = Instant::now();
        engine.reverse(&query)?;
        latencies_ms.push(item_start.elapsed().as_secs_f64() * 1000.0);
    }
    let total_secs = start.elapsed().as_secs_f64();
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50_ms = percentile(&latencies_ms, 0.50);
    let p99_ms = percentile(&latencies_ms, 0.99);
    let qps = if total_secs > 0.0 {
        count as f64 / total_secs
    } else {
        0.0
    };
    Ok(BenchReport {
        count,
        total_secs,
        p50_ms,
        p99_ms,
        qps,
    })
}

fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_ms.len() as f64 - 1.0) * p).round() as usize;
    sorted_ms
        .get(idx.min(sorted_ms.len() - 1))
        .copied()
        .unwrap_or(0.0)
}
