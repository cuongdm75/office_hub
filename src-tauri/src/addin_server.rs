// ============================================================================
// addin_server.rs
// HTTPS static-file server for Office Web Add-in (Word / Excel / PowerPoint)
//
// Replaces the separate Node.js/Vite dev-server process.
// Serves the pre-built `office-addin/dist/` tree at https://localhost:3000
// using axum-server + rustls (same dev certs as before).
// ============================================================================

use axum::{
    routing::get_service,
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

/// Port the add-in HTTPS server listens on.
/// Must match `manifest.xml` `<SourceLocation DefaultValue="https://localhost:3000/index.html"/>`
pub const ADDIN_PORT: u16 = 3000;

/// Certificate directory created by `npx office-addin-dev-certs install`.
fn certs_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".office-addin-dev-certs")
}

/// Locate the pre-built add-in UI dist directory.
///
/// * **Dev builds** (`debug_assertions`): resolved relative to the workspace root
///   so developers can iterate with `npm run build` without rebuilding Rust.
/// * **Release builds**: read from Tauri's bundled resource directory.
#[allow(unused_variables)]
pub fn resolve_dist_dir(resource_dir: Option<PathBuf>) -> Option<PathBuf> {
    // ── Release path ──────────────────────────────────────────────────────────
    #[cfg(not(debug_assertions))]
    if let Some(res) = resource_dir {
        let p = res.join("addin-ui");
        if p.join("index.html").exists() {
            return Some(p);
        }
        warn!("Release resource dir does not contain addin-ui/index.html: {:?}", p);
    }

    // ── Dev path ──────────────────────────────────────────────────────────────
    // Walk up from the exe location until we find the workspace root
    // (identified by `office-addin/dist/index.html`).
    let candidates: Vec<PathBuf> = vec![
        // Running via `cargo run` / `tauri dev` from workspace root
        std::env::current_dir()
            .ok()
            .map(|mut p| {
                if p.ends_with("src-tauri") {
                    p.pop();
                }
                p.join("office-addin").join("dist")
            })
            .unwrap_or_default(),
        // Running binary from target/debug/
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|p| p.to_path_buf()))
            .map(|p| p.join("office-addin").join("dist"))
            .unwrap_or_default(),
    ];

    for candidate in candidates {
        if candidate.join("index.html").exists() {
            return Some(candidate);
        }
    }

    None
}

/// Spawn the HTTPS add-in server on [`ADDIN_PORT`].
///
/// If the TLS certs or the dist directory are missing this function logs a
/// warning and returns early — the rest of Office Hub continues working.
pub async fn start_addin_server(dist_dir: PathBuf) {
    let certs = certs_dir();
    let cert_path = certs.join("localhost.crt");
    let key_path  = certs.join("localhost.key");

    // ── Validate prerequisites ─────────────────────────────────────────────
    if !cert_path.exists() || !key_path.exists() {
        warn!(
            "Add-in HTTPS server: TLS certs not found in {:?}. \
             Run `npx office-addin-dev-certs install` to generate them.",
            certs
        );
        return;
    }

    if !dist_dir.join("index.html").exists() {
        warn!(
            "Add-in HTTPS server: dist not found at {:?}. \
             Run `npm run build` in office-addin/ first.",
            dist_dir
        );
        return;
    }

    // ── TLS configuration ──────────────────────────────────────────────────
    let tls_config = match RustlsConfig::from_pem_file(&cert_path, &key_path).await {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Add-in HTTPS server: failed to load TLS config: {}", e);
            return;
        }
    };

    // ── Router ─────────────────────────────────────────────────────────────
    // CORS is required: Office host (Word/Excel) loads the add-in in a
    // WebView whose origin differs from localhost:3000.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // SPA fallback: unknown routes serve index.html so React Router works.
    let serve = ServeDir::new(&dist_dir)
        .fallback(ServeFile::new(dist_dir.join("index.html")));

    let app = Router::new()
        .fallback_service(get_service(serve))
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], ADDIN_PORT));
    info!(
        "Add-in HTTPS server starting on https://localhost:{} (serving {:?})",
        ADDIN_PORT, dist_dir
    );

    if let Err(e) = axum_server::bind_rustls(addr, tls_config)
        .serve(app.into_make_service())
        .await
    {
        error!("Add-in HTTPS server error: {}", e);
    }
}
