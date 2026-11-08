//! HTTP API oneshot tests.

#![expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use hexplace::api::{router, AppState};
use hexplace_core::{AddressParts, OsmType, Place};
use hexplace_engine::import::import_places;
use hexplace_engine::{Engine, EngineConfig};
use http_body_util::BodyExt;
use tower::ServiceExt;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

/// Holds the router and its temp data dir for the test lifetime.
struct TestApp {
    app: Router,
    _dir: tempfile::TempDir,
}

fn seed_app() -> TestResult<TestApp> {
    let dir = tempfile::tempdir()?;
    import_places(
        vec![Place {
            place_id: 0,
            osm_type: OsmType::Node,
            osm_id: 1,
            lat: 43.7384,
            lon: 7.4246,
            name: Some("Monaco".into()),
            display_name: String::new(),
            category: "place".into(),
            type_name: "city".into(),
            address: AddressParts {
                city: Some("Monaco".into()),
                country: Some("Monaco".into()),
                country_code: Some("mc".into()),
                ..AddressParts::default()
            },
            importance: 0.9,
        }],
        dir.path(),
    )?;
    let engine = Engine::open(EngineConfig::new(dir.path()))?;
    Ok(TestApp {
        app: router(AppState::new(engine)),
        _dir: dir,
    })
}

async fn body_json(response: axum::response::Response) -> TestResult<serde_json::Value> {
    let bytes = response.into_body().collect().await?.to_bytes();
    Ok(serde_json::from_slice(&bytes)?)
}

async fn body_bytes(response: axum::response::Response) -> TestResult<Vec<u8>> {
    Ok(response.into_body().collect().await?.to_bytes().to_vec())
}

fn json_request_body(value: &serde_json::Value) -> TestResult<Body> {
    Ok(Body::from(serde_json::to_vec(value)?))
}

/// Decodes one packed present/id/score record from a binary bulk response.
fn read_bulk_hit(bytes: &[u8], offset: usize) -> TestResult<(u8, u64, f32)> {
    let Some(record) = bytes.get(offset..offset + 13) else {
        return Err("binary bulk response must contain a full 13-byte record".into());
    };
    let present = record[0];
    let Ok(id_bytes) = <[u8; 8]>::try_from(&record[1..9]) else {
        return Err("binary hit record must include an 8-byte place_id".into());
    };
    let Ok(score_bytes) = <[u8; 4]>::try_from(&record[9..13]) else {
        return Err("binary hit record must include a 4-byte score".into());
    };
    Ok((
        present,
        u64::from_le_bytes(id_bytes),
        f32::from_le_bytes(score_bytes),
    ))
}

#[tokio::test]
async fn health_ok() -> TestResult {
    let test = seed_app()?;
    let response = test
        .app
        .oneshot(Request::builder().uri("/v1/health").body(Body::empty())?)
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "health must succeed for a seeded engine"
    );
    let json = body_json(response).await?;
    assert_eq!(json["status"], "ok", "health body must report ok status");
    Ok(())
}

#[tokio::test]
async fn status_ready_without_source_path() -> TestResult {
    let test = seed_app()?;
    let response = test
        .app
        .oneshot(Request::builder().uri("/v1/status").body(Body::empty())?)
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "status must succeed for a seeded engine"
    );
    let json = body_json(response).await?;
    assert_eq!(json["status"], "ready", "status body must report ready");
    assert!(
        json.get("source_hash").and_then(|v| v.as_str()).is_some(),
        "status must expose a source_hash string"
    );
    assert!(
        json.get("source_path").is_none(),
        "status must omit source_path from the public payload"
    );
    let Some(place_count) = json["place_count"].as_u64() else {
        return Err("status must include numeric place_count".into());
    };
    assert!(
        place_count >= 1,
        "status must report at least one imported place"
    );
    Ok(())
}

#[tokio::test]
async fn geocode_and_reverse_and_batch() -> TestResult {
    let test = seed_app()?;

    let geo = test
        .app
        .clone()
        .oneshot(Request::builder().uri("/v1/geocode?q=Monaco&limit=5").body(Body::empty())?)
        .await?;
    assert_eq!(
        geo.status(),
        StatusCode::OK,
        "geocode must succeed for the seeded place name"
    );
    let geo_json = body_json(geo).await?;
    let Some(geo_hits) = geo_json.as_array() else {
        return Err("geocode must return a JSON array".into());
    };
    assert!(
        !geo_hits.is_empty(),
        "geocode must return at least one hit for the seeded place"
    );

    let rev = test
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/reverse?lat=43.7384&lon=7.4246&limit=1")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        rev.status(),
        StatusCode::OK,
        "reverse must succeed for the seeded coordinates"
    );
    let rev_json = body_json(rev).await?;
    let Some(rev_hits) = rev_json.as_array() else {
        return Err("reverse must return a JSON array".into());
    };
    assert!(
        !rev_hits.is_empty(),
        "reverse must return at least one hit for the seeded coordinates"
    );

    let batch_body = serde_json::json!({
        "items": [
            {"op": "geocode", "id": "1", "q": "Monaco"},
            {"op": "reverse", "id": "2", "lat": 43.7384, "lon": 7.4246}
        ]
    });
    let batch = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/batch")
                .header("content-type", "application/json")
                .body(json_request_body(&batch_body)?)?,
        )
        .await?;
    assert_eq!(
        batch.status(),
        StatusCode::OK,
        "batch must succeed for mixed geocode and reverse items"
    );
    let batch_json = body_json(batch).await?;
    let Some(items) = batch_json["items"].as_array() else {
        return Err("batch response must include an items array".into());
    };
    assert_eq!(
        items.len(),
        2,
        "batch must return one result per request item"
    );
    Ok(())
}

#[tokio::test]
async fn reverse_bulk_ndjson() -> TestResult {
    let test = seed_app()?;
    let body = serde_json::json!({
        "points": [[43.7384, 7.4246], [43.7385, 7.4247]]
    });
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reverse/bulk")
                .header("content-type", "application/json")
                .header("accept", "application/x-ndjson")
                .body(json_request_body(&body)?)?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "ndjson reverse bulk must succeed for valid points"
    );
    let bytes = body_bytes(response).await?;
    let ndjson = std::str::from_utf8(&bytes)?;
    let lines: Vec<_> = ndjson.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "ndjson reverse bulk must emit one line per input point"
    );
    let first: serde_json::Value = serde_json::from_str(lines[0])?;
    assert!(
        first.get("place_id").is_some() || first.get("error").is_some(),
        "each ndjson line must carry a place_id or error"
    );
    Ok(())
}

#[tokio::test]
async fn reverse_bulk_binary_present_flag() -> TestResult {
    let test = seed_app()?;
    // Hit near Monaco, then a mid-ocean miss.
    let body = serde_json::json!({
        "points": [[43.7384, 7.4246], [0.0, -150.0]]
    });
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reverse/bulk")
                .header("content-type", "application/json")
                .header("accept", "application/octet-stream")
                .body(json_request_body(&body)?)?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "binary reverse bulk must succeed for mixed hit and miss points"
    );
    let bytes = body_bytes(response).await?;
    assert_eq!(
        bytes.len(),
        26,
        "binary reverse bulk must emit 13 bytes per point"
    );

    let (hit_present, hit_id, hit_score) = read_bulk_hit(&bytes, 0)?;
    assert_eq!(
        hit_present, 1,
        "seeded coordinates must yield a present hit"
    );
    assert_eq!(hit_id, 0, "seeded place must keep fixture place_id zero");
    assert!(
        hit_score.is_finite() && hit_score > 0.0,
        "present hit must carry a finite positive score"
    );

    let (miss_present, miss_id, miss_score) = read_bulk_hit(&bytes, 13)?;
    assert_eq!(
        miss_present, 0,
        "ocean coordinates must yield an absent hit"
    );
    assert_eq!(miss_id, 0, "absent hit must zero the place_id field");
    assert_eq!(
        miss_score.to_bits(),
        0f32.to_bits(),
        "absent hit must zero the score field"
    );
    Ok(())
}

#[tokio::test]
async fn batch_empty_items_is_bad_request() -> TestResult {
    let test = seed_app()?;
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/batch")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"items":[]}"#))?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "empty batch items must be rejected as a client error"
    );
    Ok(())
}

#[tokio::test]
async fn reverse_bulk_json_over_cap_is_bad_request() -> TestResult {
    let test = seed_app()?;
    let over = Engine::MAX_BATCH_ITEMS + 1;
    let mut body = String::from("{\"points\":[");
    for i in 0..over {
        if i > 0 {
            body.push(',');
        }
        body.push_str("[0,0]");
    }
    body.push_str("]}");
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reverse/bulk")
                .header("content-type", "application/json")
                .header("accept", "application/x-ndjson")
                .body(Body::from(body))?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "JSON reverse-bulk over MAX_BATCH_ITEMS must be rejected during capped deserialize"
    );
    let json = body_json(response).await?;
    let Some(message) = json["error"].as_str() else {
        return Err("error responses must include an error string".into());
    };
    assert!(
        message.contains("limited"),
        "over-cap reverse-bulk error must mention the limit: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn reverse_bulk_empty_points_is_bad_request() -> TestResult {
    let test = seed_app()?;
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reverse/bulk")
                .header("content-type", "application/json")
                .header("accept", "application/x-ndjson")
                .body(Body::from(r#"{"points":[]}"#))?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "empty reverse-bulk points must be rejected as a client error"
    );
    let json = body_json(response).await?;
    let Some(message) = json["error"].as_str() else {
        return Err("error responses must include an error string".into());
    };
    assert!(
        message.contains("empty"),
        "empty reverse-bulk error must mention emptiness"
    );
    Ok(())
}

#[tokio::test]
async fn reverse_bulk_binary_request_ndjson_hit() -> TestResult {
    let test = seed_app()?;
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&43.7384f32.to_le_bytes());
    body.extend_from_slice(&7.4246f32.to_le_bytes());
    let response = test
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reverse/bulk")
                .header("content-type", "application/octet-stream")
                .header("accept", "application/x-ndjson")
                .body(Body::from(body))?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "binary request with ndjson accept must succeed near the seeded place"
    );
    let bytes = body_bytes(response).await?;
    let ndjson = std::str::from_utf8(&bytes)?;
    let Some(line) = ndjson.lines().next() else {
        return Err("ndjson bulk must yield at least one line".into());
    };
    let first: serde_json::Value = serde_json::from_str(line)?;
    assert!(
        first.get("place_id").is_some(),
        "binary request near the seeded place must return a place_id"
    );
    Ok(())
}
