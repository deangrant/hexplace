//! Unit tests for query validation.

use geofind_core::{GeoPoint, ReverseQuery, SearchQuery};

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
fn geo_point_rejects_bad_lat() {
    assert!(GeoPoint::new(100.0, 0.0).is_err());
}

#[test]
fn reverse_defaults_limit_to_one() {
    let q = ReverseQuery::new(1.0, 2.0, None).unwrap();
    assert_eq!(q.limit, 1);
}
