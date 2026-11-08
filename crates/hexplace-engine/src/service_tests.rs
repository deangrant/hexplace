//! Unit tests for geocoding service helpers.

use super::*;

/// Builds error results for a panicked worker's index range.
fn batch_worker_error(
    items: &[BatchItem],
    range: std::ops::Range<usize>,
    message: &str,
) -> Vec<(usize, BatchResult)> {
    range
        .filter_map(|idx| items.get(idx).map(|item| (idx, batch_item_error(item, message))))
        .collect()
}

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
    assert_eq!(
        filled.len(),
        2,
        "worker error helper must cover the full requested range"
    );
    assert_eq!(filled[0].0, 1, "first filled index must be range start");
    assert_eq!(
        filled[0].1.id.as_deref(),
        Some("b"),
        "panic filler must preserve the reverse item id"
    );
    assert_eq!(
        filled[0].1.error.as_deref(),
        Some("batch worker panicked"),
        "panic filler must surface the worker panic message"
    );
    assert!(
        filled[0].1.results.is_empty(),
        "panic filler must not invent geocode hits"
    );
    assert_eq!(filled[1].0, 2, "second filled index must follow the range");
    assert_eq!(
        filled[1].1.id, None,
        "items without ids must keep a None id after panic fill"
    );
    assert_eq!(
        filled[1].1.error.as_deref(),
        Some("batch worker panicked"),
        "panic filler must surface the worker panic message"
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
    assert_eq!(
        best,
        vec![(0.9, 9), (0.7, 7), (0.5, 5)],
        "push_best must keep the highest scores in descending order"
    );
}
