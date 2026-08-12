//! Axum HTTP API for Geofind.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use geofind_core::{
    BatchRequest, BatchResponse, CoreError, Geocoder, PlaceHit, ReverseQuery, SearchQuery,
};
use geofind_engine::Engine;
use serde::{Deserialize, Serialize};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    engine: Arc<Engine>,
}

impl AppState {
    /// Wraps an engine for HTTP handlers.
    pub fn new(engine: Engine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }
}

/// Builds the `/v1` router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/geocode", get(geocode))
        .route("/v1/reverse", get(reverse))
        .route("/v1/batch", post(batch))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .with_state(state)
}

/// Serves the API until shutdown.
pub async fn serve(engine: Engine, bind: SocketAddr) -> Result<(), CoreError> {
    let state = AppState::new(engine);
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| CoreError::io(format!("bind {bind}: {e}")))?;
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| CoreError::io(format!("server error: {e}")))?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
    schema_version: u32,
    place_count: u64,
    source_path: String,
    h3_resolution: u8,
    built_at_unix: u64,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let m = state.engine.manifest();
    Json(StatusResponse {
        status: "ready",
        schema_version: m.schema_version,
        place_count: m.place_count,
        source_path: m.source_path.clone(),
        h3_resolution: m.h3_resolution,
        built_at_unix: m.built_at_unix,
    })
}

#[derive(Debug, Deserialize)]
struct GeocodeParams {
    q: String,
    limit: Option<usize>,
}

async fn geocode(
    State(state): State<AppState>,
    Query(params): Query<GeocodeParams>,
) -> Result<Json<Vec<PlaceHit>>, ApiError> {
    let query = SearchQuery::new(params.q, params.limit)?;
    let hits = state.engine.geocode(&query)?;
    Ok(Json(hits))
}

#[derive(Debug, Deserialize)]
struct ReverseParams {
    lat: f64,
    lon: f64,
    limit: Option<usize>,
}

async fn reverse(
    State(state): State<AppState>,
    Query(params): Query<ReverseParams>,
) -> Result<Json<Vec<PlaceHit>>, ApiError> {
    let query = ReverseQuery::new(params.lat, params.lon, params.limit)?;
    let hits = state.engine.reverse(&query)?;
    Ok(Json(hits))
}

async fn batch(
    State(state): State<AppState>,
    Json(body): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, ApiError> {
    let response = state.engine.batch(&body)?;
    Ok(Json(response))
}

#[derive(Debug)]
struct ApiError(CoreError);

impl From<CoreError> for ApiError {
    fn from(value: CoreError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            CoreError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            CoreError::NotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}
