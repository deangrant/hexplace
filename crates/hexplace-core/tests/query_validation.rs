//! Unit tests for query validation.

use hexplace_core::{GeoPoint, ReverseQuery, SearchQuery};

#[test]
fn search_rejects_empty_query() {
    assert!(SearchQuery::new("   ", None).is_err());
}

#[test]
fn search_clamps_limit() {
    let q = SearchQuery::new("paris", Some(999)).unwrap();
    assert_eq!(q.limit, SearchQuery::MAX_LIMIT);
}

#[test]
fn search_rejects_zero_limit() {
    let err = SearchQuery::new("paris", Some(0)).unwrap_err();
    assert!(err.to_string().contains("limit must be at least 1"));
}

#[test]
fn geo_point_rejects_bad_lat() {
    assert!(GeoPoint::new(100.0, 0.0).is_err());
}

#[test]
fn reverse_defaults_limit_to_one() {
    let q = ReverseQuery::new(1.0, 2.0, None).unwrap();
    assert_eq!(q.limit, 1);
}

#[test]
fn reverse_rejects_zero_limit() {
    let err = ReverseQuery::new(1.0, 2.0, Some(0)).unwrap_err();
    assert!(err.to_string().contains("limit must be at least 1"));
}
