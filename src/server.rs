use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::{path::PathBuf, sync::Arc};
use tracing::{info, warn};
use anyhow::Result;

use crate::sync::{build_manifest, compute_diff, DiffRequest};

#[derive(Clone)]
struct AppState {
    token: String,
    root: PathBuf,
}

/// Authenticate request via Bearer token
fn auth(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == expected)
        .unwrap_or(false)
}

/// GET /manifest — return server's current file manifest
async fn handle_manifest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response();
    }

    match build_manifest(&state.root) {
        Ok(manifest) => (StatusCode::OK, Json(manifest)).into_response(),
        Err(e) => {
            warn!("Failed to build manifest: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

/// POST /diff — client sends its manifest, server returns diff
async fn handle_diff(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DiffRequest>,
) -> impl IntoResponse {
    if !auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response();
    }

    let server_manifest = match build_manifest(&state.root) {
        Ok(m) => m,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
        }
    };

    let diff = compute_diff(&req.manifest, &server_manifest);
    info!(
        "Diff: {} files needed, {} orphaned",
        diff.needed.len(),
        diff.orphaned.len()
    );

    (StatusCode::OK, Json(diff)).into_response()
}

/// POST /file/:path — receive a single file
async fn handle_file_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rel_path): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if !auth(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Sanitize path - prevent directory traversal
    let rel_path = rel_path.replace("..", "").trim_start_matches('/').to_string();
    let dest = state.root.join(&rel_path);

    // Create parent dirs
    if let Some(parent) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Failed to create dir {:?}: {}", parent, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    match std::fs::write(&dest, &body) {
        Ok(_) => {
            info!("Received: {} ({} bytes)", rel_path, body.len());
            StatusCode::OK.into_response()
        }
        Err(e) => {
            warn!("Failed to write {}: {}", rel_path, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /file/:path — download a file
async fn handle_file_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rel_path): Path<String>,
) -> impl IntoResponse {
    if !auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, vec![]).into_response();
    }

    let rel_path = rel_path.replace("..", "").trim_start_matches('/').to_string();
    let target = state.root.join(&rel_path);

    match std::fs::read(&target) {
        Ok(contents) => {
            info!("Serving: {} ({} bytes)", rel_path, contents.len());
            (StatusCode::OK, contents).into_response()
        }
        Err(e) => {
            warn!("Failed to read {}: {}", rel_path, e);
            (StatusCode::NOT_FOUND, vec![]).into_response()
        }
    }
}

/// DELETE /file/:path — delete an orphaned file
async fn handle_file_delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rel_path): Path<String>,
) -> impl IntoResponse {
    if !auth(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let rel_path = rel_path.replace("..", "").trim_start_matches('/').to_string();
    let target = state.root.join(&rel_path);

    match std::fs::remove_file(&target) {
        Ok(_) => {
            info!("Deleted orphan: {}", rel_path);
            StatusCode::OK.into_response()
        }
        Err(e) => {
            warn!("Failed to delete {}: {}", rel_path, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// GET /health
async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}))
}

pub async fn run(port: u16, token: String, dir: String) -> Result<()> {
    let root = PathBuf::from(&dir).canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&dir));

    info!("syncai server starting on :{}", port);
    info!("Serving directory: {:?}", root);

    let state = Arc::new(AppState { token, root });

    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/manifest", get(handle_manifest))
        .route("/diff", post(handle_diff))
        .route("/file/{*path}", get(handle_file_download))
        .route("/file/{*path}", post(handle_file_upload))
        .route("/file/{*path}", axum::routing::delete(handle_file_delete))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Listening on 0.0.0.0:{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
