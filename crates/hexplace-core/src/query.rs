//! Query and batch request types.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::place::PlaceHit;

/// WGS84 geographic coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

impl GeoPoint {
    /// Creates a point after validating latitude and longitude ranges.
    pub fn new(lat: f64, lon: f64) -> Result<Self, CoreError> {
        if !lat.is_finite() {
            return Err(CoreError::invalid(format!(
                "latitude must be finite: {lat}"
            )));
        }
        if !lon.is_finite() {
            return Err(CoreError::invalid(format!(
                "longitude must be finite: {lon}"
            )));
        }
        if !(-90.0..=90.0).contains(&lat) {
            return Err(CoreError::invalid(format!("latitude out of range: {lat}")));
        }
        if !(-180.0..=180.0).contains(&lon) {
            return Err(CoreError::invalid(format!("longitude out of range: {lon}")));
        }
        Ok(Self { lat, lon })
    }
}

/// Forward geocoding query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Free-text query string.
    pub q: String,
    /// Maximum number of results to return.
    pub limit: usize,
}

impl SearchQuery {
    /// Default maximum result count.
    pub const DEFAULT_LIMIT: usize = 10;
    /// Hard upper bound on result count.
    pub const MAX_LIMIT: usize = 50;

    /// Creates a query after normalizing the text and validating the limit.
    ///
    /// Omitted limits use [`Self::DEFAULT_LIMIT`]. Explicit `0` is rejected;
    /// values above [`Self::MAX_LIMIT`] are capped.
    pub fn new(q: impl Into<String>, limit: Option<usize>) -> Result<Self, CoreError> {
        let q = q.into();
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return Err(CoreError::invalid("query must not be empty"));
        }
        let limit = match limit {
            None => Self::DEFAULT_LIMIT,
            Some(0) => {
                return Err(CoreError::invalid("limit must be at least 1"));
            }
            Some(n) => n.min(Self::MAX_LIMIT),
        };
        Ok(Self {
            q: trimmed.to_owned(),
            limit,
        })
    }
}

/// Reverse geocoding query.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReverseQuery {
    /// Query coordinate.
    pub point: GeoPoint,
    /// Maximum number of results to return.
    pub limit: usize,
}

impl ReverseQuery {
    /// Default maximum result count for reverse lookup.
    pub const DEFAULT_LIMIT: usize = 1;
    /// Hard upper bound on result count.
    pub const MAX_LIMIT: usize = 50;

    /// Creates a reverse query with a validated limit.
    ///
    /// Omitted limits use [`Self::DEFAULT_LIMIT`]. Explicit `0` is rejected;
    /// values above [`Self::MAX_LIMIT`] are capped.
    pub fn new(lat: f64, lon: f64, limit: Option<usize>) -> Result<Self, CoreError> {
        let point = GeoPoint::new(lat, lon)?;
        let limit = match limit {
            None => Self::DEFAULT_LIMIT,
            Some(0) => {
                return Err(CoreError::invalid("limit must be at least 1"));
            }
            Some(n) => n.min(Self::MAX_LIMIT),
        };
        Ok(Self { point, limit })
    }
}

/// One item in a bulk request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BatchItem {
    /// Forward geocode this text.
    Geocode {
        /// Optional caller-supplied correlation id.
        #[serde(default)]
        id: Option<String>,
        /// Free-text query.
        q: String,
        /// Optional result limit.
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Reverse geocode this coordinate.
    Reverse {
        /// Optional caller-supplied correlation id.
        #[serde(default)]
        id: Option<String>,
        /// Latitude in degrees.
        lat: f64,
        /// Longitude in degrees.
        lon: f64,
        /// Optional result limit.
        #[serde(default)]
        limit: Option<usize>,
    },
}

/// Bulk geocoding request body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRequest {
    /// Items to process in order.
    pub items: Vec<BatchItem>,
}

/// Result for a single batch item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResult {
    /// Optional correlation id echoed from the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Hits for this item when successful.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<PlaceHit>,
    /// Error message when this item failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Bulk geocoding response body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResponse {
    /// Per-item results aligned with the request order.
    pub items: Vec<BatchResult>,
}

/// Bulk reverse request body (`POST /v1/reverse/bulk`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverseBulkRequest {
    /// Points to reverse as `[lat, lon]` pairs.
    pub points: Vec<[f64; 2]>,
}

/// Compact reverse hit for bulk NDJSON/binary responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverseBulkHit {
    /// Optional index into the request points array.
    pub index: usize,
    /// Winning place id when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_id: Option<u64>,
    /// Ranking score when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    /// Display name when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Error when this point failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
