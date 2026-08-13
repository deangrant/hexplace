//! Axum HTTP API for Hexplace.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hexplace_core::{
    BatchRequest, BatchResponse, CoreError, Geocoder, PlaceHit, ReverseBulkHit, ReverseBulkRequest,
    ReverseQuery, SearchQuery,
};
use hexplace_engine::Engine;
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

/// Max JSON body size for bulk endpoints.
const BATCH_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
/// Packed reverse-bulk hit: `u8 present` + `u64 place_id` + `f32 score`.
const BINARY_BULK_HIT_BYTES: usize = 13;
/// Request timeout for lightweight endpoints.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Request timeout for large batch jobs.
const BATCH_REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// Builds the `/v1` router.
pub fn router(state: AppState) -> Router {
    let batch_routes = Router::new()
        .route(
            "/v1/batch",
            post(batch).layer(DefaultBodyLimit::max(BATCH_BODY_LIMIT_BYTES)),
        )
        .route(
            "/v1/reverse/bulk",
            post(reverse_bulk).layer(DefaultBodyLimit::max(BATCH_BODY_LIMIT_BYTES)),
        )
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            BATCH_REQUEST_TIMEOUT,
        ));

    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/geocode", get(geocode))
        .route("/v1/reverse", get(reverse))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            DEFAULT_REQUEST_TIMEOUT,
        ))
        .merge(batch_routes)
        .layer(TraceLayer::new_for_http())
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
    source_hash: String,
    h3_resolution_fine: u8,
    h3_resolution_coarse: u8,
    store_format: String,
    spatial_format: String,
    built_at_unix: u64,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let m = state.engine.manifest();
    Json(StatusResponse {
        status: "ready",
        schema_version: m.schema_version,
        place_count: m.place_count,
        source_hash: m.source_hash.clone(),
        h3_resolution_fine: m.h3_resolution_fine,
        h3_resolution_coarse: m.h3_resolution_coarse,
        store_format: m.store_format.clone(),
        spatial_format: m.spatial_format.clone(),
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
    let engine = Arc::clone(&state.engine);
    let response = tokio::task::spawn_blocking(move || engine.batch(&body))
        .await
        .map_err(|e| CoreError::io(format!("batch worker failed: {e}")))??;
    Ok(Json(response))
}

async fn reverse_bulk(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-ndjson");
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    let bytes = axum::body::to_bytes(body, BATCH_BODY_LIMIT_BYTES)
        .await
        .map_err(|e| CoreError::invalid(format!("failed to read body: {e}")))?;

    let points = if content_type.contains("octet-stream") {
        parse_binary_points(&bytes)?
    } else {
        let req: ReverseBulkRequest = serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::invalid(format!("invalid bulk JSON: {e}")))?;
        if req.points.is_empty() {
            return Err(CoreError::invalid("points must not be empty").into());
        }
        if req.points.len() > Engine::MAX_BATCH_ITEMS {
            return Err(CoreError::invalid(format!(
                "bulk limited to {} points",
                Engine::MAX_BATCH_ITEMS
            ))
            .into());
        }
        req.points
    };

    let engine = Arc::clone(&state.engine);
    let hits = tokio::task::spawn_blocking(move || run_reverse_bulk(&engine, &points))
        .await
        .map_err(|e| CoreError::io(format!("bulk worker failed: {e}")))??;

    if accept.contains("octet-stream") {
        Ok(binary_bulk_response(&hits))
    } else {
        Ok(ndjson_bulk_response(&hits)?)
    }
}

fn parse_binary_points(bytes: &[u8]) -> Result<Vec<[f64; 2]>, CoreError> {
    if bytes.len() % 8 != 0 {
        return Err(CoreError::invalid(
            "binary bulk body must be packed f32 lat/lon pairs",
        ));
    }
    let count = bytes.len() / 8;
    if count == 0 {
        return Err(CoreError::invalid("points must not be empty"));
    }
    if count > Engine::MAX_BATCH_ITEMS {
        return Err(CoreError::invalid(format!(
            "bulk limited to {} points",
            Engine::MAX_BATCH_ITEMS
        )));
    }
    let mut points = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * 8;
        let lat = f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as f64;
        let lon = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as f64;
        points.push([lat, lon]);
    }
    Ok(points)
}

fn run_reverse_bulk(
    engine: &Engine,
    points: &[[f64; 2]],
) -> Result<Vec<ReverseBulkHit>, CoreError> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(points.len())
        .max(1);
    let chunk_size = (points.len() + workers - 1) / workers;
    let mut slots: Vec<Option<ReverseBulkHit>> = (0..points.len()).map(|_| None).collect();

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for (chunk_idx, chunk) in points.chunks(chunk_size).enumerate() {
            let start = chunk_idx * chunk_size;
            let end = start + chunk.len();
            handles.push((
                start..end,
                scope.spawn(move || {
                    let mut local = Vec::with_capacity(chunk.len());
                    for (offset, point) in chunk.iter().enumerate() {
                        let index = start + offset;
                        let hit = match ReverseQuery::new(point[0], point[1], Some(1)) {
                            Ok(query) => match engine.reverse(&query) {
                                Ok(results) => {
                                    if let Some(top) = results.first() {
                                        ReverseBulkHit {
                                            index,
                                            place_id: Some(top.place_id),
                                            score: Some(top.score),
                                            display_name: Some(top.display_name.clone()),
                                            error: None,
                                        }
                                    } else {
                                        ReverseBulkHit {
                                            index,
                                            place_id: None,
                                            score: None,
                                            display_name: None,
                                            error: Some("no results".into()),
                                        }
                                    }
                                }
                                Err(e) => ReverseBulkHit {
                                    index,
                                    place_id: None,
                                    score: None,
                                    display_name: None,
                                    error: Some(e.to_string()),
                                },
                            },
                            Err(e) => ReverseBulkHit {
                                index,
                                place_id: None,
                                score: None,
                                display_name: None,
                                error: Some(e.to_string()),
                            },
                        };
                        local.push((index, hit));
                    }
                    local
                }),
            ));
        }
        for (range, handle) in handles {
            match handle.join() {
                Ok(local) => {
                    for (idx, hit) in local {
                        slots[idx] = Some(hit);
                    }
                }
                Err(_) => {
                    for idx in range {
                        slots[idx] = Some(ReverseBulkHit {
                            index: idx,
                            place_id: None,
                            score: None,
                            display_name: None,
                            error: Some("bulk worker panicked".into()),
                        });
                    }
                }
            }
        }
    });

    Ok(slots
        .into_iter()
        .enumerate()
        .map(|(idx, slot)| {
            slot.unwrap_or_else(|| ReverseBulkHit {
                index: idx,
                place_id: None,
                score: None,
                display_name: None,
                error: Some("bulk worker panicked".into()),
            })
        })
        .collect())
}

fn ndjson_bulk_response(hits: &[ReverseBulkHit]) -> Result<Response, ApiError> {
    let mut out = String::new();
    for hit in hits {
        let line = serde_json::to_string(hit)
            .map_err(|e| CoreError::io(format!("ndjson encode failed: {e}")))?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(([(header::CONTENT_TYPE, "application/x-ndjson")], out).into_response())
}

fn binary_bulk_response(hits: &[ReverseBulkHit]) -> Response {
    let mut bytes = Vec::with_capacity(hits.len() * BINARY_BULK_HIT_BYTES);
    for hit in hits {
        match (hit.place_id, hit.score) {
            (Some(id), Some(score)) => {
                bytes.push(1);
                bytes.extend_from_slice(&id.to_le_bytes());
                bytes.extend_from_slice(&score.to_le_bytes());
            }
            _ => {
                bytes.push(0);
                bytes.extend_from_slice(&0u64.to_le_bytes());
                bytes.extend_from_slice(&0f32.to_le_bytes());
            }
        }
    }
    ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
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
