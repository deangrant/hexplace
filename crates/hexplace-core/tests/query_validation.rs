//! Unit tests for query validation.

use hexplace_core::{GeoPoint, ReverseQuery, SearchQuery};

#[test]
fn search_rejects_empty_query() {
    assert!(
        SearchQuery::new("   ", None).is_err(),
        "whitespace-only query must fail validation"
    );
}

#[test]
fn search_clamps_limit() {
    let result = SearchQuery::new("paris", Some(999));
    assert!(
        result.is_ok(),
        "oversized limit must still construct after clamping"
    );
    let Ok(q) = result else {
        return;
    };
    assert_eq!(
        q.limit,
        SearchQuery::MAX_LIMIT,
        "limits above the hard cap must clamp to MAX_LIMIT"
    );
}

#[test]
fn search_rejects_zero_limit() {
    let result = SearchQuery::new("paris", Some(0));
    assert!(result.is_err(), "explicit zero limit must fail validation");
    let Err(err) = result else {
        return;
    };
    assert!(
        err.to_string().contains("limit must be at least 1"),
        "zero-limit rejection must state the minimum allowed value"
    );
}

#[test]
fn geo_point_rejects_bad_lat() {
    assert!(
        GeoPoint::new(100.0, 0.0).is_err(),
        "latitude above 90 degrees must fail validation"
    );
}

#[test]
fn reverse_defaults_limit_to_one() {
    let result = ReverseQuery::new(1.0, 2.0, None);
    assert!(
        result.is_ok(),
        "valid coordinates with omitted limit must construct"
    );
    let Ok(q) = result else {
        return;
    };
    assert_eq!(
        q.limit,
        ReverseQuery::DEFAULT_LIMIT,
        "omitted reverse limit must use DEFAULT_LIMIT"
    );
}

#[test]
fn reverse_rejects_zero_limit() {
    let result = ReverseQuery::new(1.0, 2.0, Some(0));
    assert!(
        result.is_err(),
        "explicit zero reverse limit must fail validation"
    );
    let Err(err) = result else {
        return;
    };
    assert!(
        err.to_string().contains("limit must be at least 1"),
        "zero-limit rejection must state the minimum allowed value"
    );
}

#[test]
fn geo_point_rejects_non_finite() {
    assert!(
        GeoPoint::new(f64::NAN, 0.0).is_err(),
        "NaN latitude must fail validation"
    );
    assert!(
        GeoPoint::new(0.0, f64::INFINITY).is_err(),
        "infinite longitude must fail validation"
    );
    assert!(
        GeoPoint::new(f64::NEG_INFINITY, 0.0).is_err(),
        "negative-infinite latitude must fail validation"
    );
    assert!(
        ReverseQuery::new(f64::NAN, 1.0, None).is_err(),
        "NaN reverse latitude must fail validation"
    );
}

#[test]
fn geo_point_accepts_antimeridian_longitude() {
    assert!(
        GeoPoint::new(0.0, 180.0).is_ok(),
        "longitude 180 must be accepted as the antimeridian"
    );
    assert!(
        GeoPoint::new(0.0, -180.0).is_ok(),
        "longitude -180 must be accepted as the antimeridian"
    );
}

#[test]
fn search_accepts_large_query_string() {
    let q = "a".repeat(100_000);
    let result = SearchQuery::new(q, Some(1));
    assert!(result.is_ok(), "large free-text queries must be accepted");
    let Ok(query) = result else {
        return;
    };
    assert_eq!(
        query.q.len(),
        100_000,
        "accepted query text length must be preserved after construction"
    );
}
