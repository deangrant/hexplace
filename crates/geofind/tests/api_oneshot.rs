//! HTTP API oneshot tests.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use geofind::api::{router, AppState};
use geofind_core::{AddressParts, OsmType, Place};
use geofind_engine::import::import_places;
use geofind_engine::{Engine, EngineConfig};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn seed_engine() -> Engine {
    let dir = tempfile::tempdir().unwrap();
    // Leak the tempdir for the duration of the process so paths stay valid.
    let path = dir.keep();
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
        &path,
    )
    .unwrap();
    Engine::open(EngineConfig::new(path)).unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_ok() {
    let app = router(AppState::new(seed_engine()));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn geocode_and_reverse_and_batch() {
    let app = router(AppState::new(seed_engine()));

    let geo = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/geocode?q=Monaco&limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(geo.status(), StatusCode::OK);
    let geo_json = body_json(geo).await;
    assert!(geo_json.as_array().unwrap().len() >= 1);

    let rev = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/reverse?lat=43.7384&lon=7.4246&limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rev.status(), StatusCode::OK);
    let rev_json = body_json(rev).await;
    assert!(rev_json.as_array().unwrap().len() >= 1);

    let batch_body = serde_json::json!({
        "items": [
            {"op": "geocode", "id": "1", "q": "Monaco"},
            {"op": "reverse", "id": "2", "lat": 43.7384, "lon": 7.4246}
        ]
    });
    let batch = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/batch")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(batch.status(), StatusCode::OK);
    let batch_json = body_json(batch).await;
    assert_eq!(batch_json["items"].as_array().unwrap().len(), 2);
}
