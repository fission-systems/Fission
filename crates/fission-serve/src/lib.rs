//! `fission-serve` — multi-user decompilation HTTP API server.
//!
//! Inspired by Ghidra Server: a single `fission-serve` instance can serve
//! multiple simultaneous analysts. Each client uploads a binary and receives a
//! `session_id`; all subsequent requests are scoped to that session.
//!
//! # Quick start
//!
//! ```bash
//! fission-serve --host 0.0.0.0 --port 7331
//! ```
//!
//! Or via `fission_cli`:
//!
//! ```bash
//! fission_cli serve --host 0.0.0.0 --port 7331
//! ```
//!
//! # REST API
//!
//! | Method | Path                              | Description                        |
//! |--------|-----------------------------------|------------------------------------|
//! | GET    | /api/status                       | Server version + active sessions   |
//! | POST   | /api/binary                       | Upload binary → returns session_id |
//! | GET    | /api/functions/:session           | Function list for session          |
//! | POST   | /api/decompile/:session/:addr     | Decompile function at hex addr     |
//! | GET    | /api/xrefs/:session/:addr         | Cross-references for function      |
//! | DELETE | /api/session/:session             | Release session explicitly         |

pub mod config;
pub mod session;
pub mod types;
mod handlers;

use anyhow::Result;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    routing::{delete, get, post},
};
use std::{net::SocketAddr, sync::Arc};
use tower_http::cors::CorsLayer;
use tracing::info;

pub use config::ServeConfig;
pub use session::SessionStore;

/// Start the fission HTTP API server with the given configuration.
/// Blocks until the server is shut down.
pub async fn run_serve(config: ServeConfig) -> Result<()> {
    let store = Arc::new(SessionStore::new(config.max_sessions, config.session_ttl_secs));

    // Start background TTL sweeper
    {
        let store = store.clone();
        tokio::spawn(async move {
            store.run_sweeper().await;
        });
    }

    let cors = build_cors(&config.allowed_origins);

    let app = Router::new()
        .route("/api/status",                    get(handlers::status::handle_status))
        .route("/api/binary",                    post(handlers::binary::handle_upload_binary))
        .route("/api/functions/:session",        get(handlers::functions::handle_list_functions))
        .route("/api/decompile/:session/:addr",  post(handlers::decompile::handle_decompile))
        .route("/api/xrefs/:session/:addr",      get(handlers::xrefs::handle_xrefs))
        .route("/api/session/:session",          delete(handlers::binary::handle_delete_session))
        .with_state(store)
        .layer(cors)
        .layer(DefaultBodyLimit::max(config.max_upload_bytes));

    let addr = SocketAddr::new(config.host.parse()?, config.port);
    info!("fission-serve  →  http://{}:{}", config.host, config.port);
    info!("Max sessions: {}  |  Session TTL: {}s  |  Upload limit: {}MB",
        config.max_sessions,
        config.session_ttl_secs,
        config.max_upload_bytes / 1024 / 1024,
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_cors(allowed_origins: &[String]) -> CorsLayer {
    let mut cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any);

    for origin in allowed_origins {
        if let Ok(v) = origin.parse::<HeaderValue>() {
            cors = cors.allow_origin(v);
        }
    }
    cors
}
