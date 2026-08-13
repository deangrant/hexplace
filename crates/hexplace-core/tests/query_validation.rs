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

#[test]
fn geo_point_rejects_non_finite() {
    assert!(GeoPoint::new(f64::NAN, 0.0).is_err());
    assert!(GeoPoint::new(0.0, f64::INFINITY).is_err());
    assert!(GeoPoint::new(f64::NEG_INFINITY, 0.0).is_err());
    assert!(ReverseQuery::new(f64::NAN, 1.0, None).is_err());
}

#[test]
fn geo_point_accepts_antimeridian_longitude() {
    assert!(GeoPoint::new(0.0, 180.0).is_ok());
    assert!(GeoPoint::new(0.0, -180.0).is_ok());
}

#[test]
fn search_accepts_large_query_string() {
    let q = "a".repeat(100_000);
    let query = SearchQuery::new(q, Some(1)).unwrap();
    assert_eq!(query.q.len(), 100_000);
}
