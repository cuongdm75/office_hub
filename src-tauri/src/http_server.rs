use axum::{
    routing::{get, get_service, post, delete},
    Router, Json, extract::{Multipart, State, Path as AxumPath},
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, error};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    public_dir: PathBuf,
}

async fn handle_upload(mut multipart: Multipart) -> Json<Value> {
    let upload_dir = std::env::temp_dir().join("office_hub_uploads");
    if let Err(e) = std::fs::create_dir_all(&upload_dir) {
        return Json(serde_json::json!({ "error": format!("Failed to create upload dir: {}", e) }));
    }

    let mut saved_path = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = field.file_name().unwrap_or("upload.tmp").to_string();
        let unique_name = format!("{}_{}", uuid::Uuid::new_v4(), file_name);
        let file_path = upload_dir.join(&unique_name);
        
        if let Ok(data) = field.bytes().await {
            if let Err(e) = std::fs::write(&file_path, &data) {
                return Json(serde_json::json!({ "error": format!("Failed to write file: {}", e) }));
            }
            saved_path = Some(file_path);
            break; // Just handle the first file for now
        }
    }

    if let Some(path) = saved_path {
        Json(serde_json::json!({ "file_path": path.to_string_lossy().to_string() }))
    } else {
        Json(serde_json::json!({ "error": "No file received" }))
    }
}

async fn list_artifacts(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&state.public_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let timestamp = metadata.modified()
                        .ok()
                        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                        .unwrap_or_default();
                    files.push(serde_json::json!({
                        "id": file_name.clone(),
                        "name": file_name,
                        "url": format!("/files/{}", file_name),
                        "timestamp": timestamp,
                        "size": metadata.len(),
                    }));
                }
            }
        }
    }
    // Sort descending by timestamp (newest first)
    files.sort_by(|a, b| {
        let ta = a["timestamp"].as_str().unwrap_or("");
        let tb = b["timestamp"].as_str().unwrap_or("");
        tb.cmp(ta)
    });
    Json(serde_json::json!({ "artifacts": files }))
}

async fn delete_artifact(State(state): State<Arc<AppState>>, AxumPath(filename): AxumPath<String>) -> Json<Value> {
    let file_path = state.public_dir.join(&filename);
    // Basic security check to prevent directory traversal
    if !file_path.starts_with(&state.public_dir) || filename.contains("..") || filename.contains("/") || filename.contains("\\") {
        return Json(serde_json::json!({ "error": "Invalid filename" }));
    }

    if file_path.exists() {
        if let Err(e) = std::fs::remove_file(&file_path) {
            return Json(serde_json::json!({ "error": format!("Failed to delete: {}", e) }));
        }
        Json(serde_json::json!({ "success": true }))
    } else {
        Json(serde_json::json!({ "error": "File not found" }))
    }
}

pub async fn start_server(port: u16, public_dir: std::path::PathBuf) {
    // Ensure the public directory exists
    if let Err(e) = std::fs::create_dir_all(&public_dir) {
        error!("Failed to create public directory for HTTP server: {}", e);
        return;
    }

    let state = Arc::new(AppState {
        public_dir: public_dir.clone(),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest_service("/files", get_service(ServeDir::new(public_dir)))
        .route("/upload", post(handle_upload))
        .route("/api/artifacts", get(list_artifacts))
        .route("/api/artifacts/{filename}", delete(delete_artifact))
        .with_state(state)
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting HTTP file server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await;
    match listener {
        Ok(l) => {
            if let Err(e) = axum::serve(l, app).await {
                error!("HTTP server error: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to bind HTTP server to {}: {}", addr, e);
        }
    }
}
