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
    Extension, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json,
};
use std::{net::SocketAddr, sync::Arc};
use tower_http::cors::CorsLayer;
use tracing::info;

pub use config::ServeConfig;
pub use session::SessionStore;

#[derive(Debug, Clone, Copy)]
pub struct ServeRuntimeInfo {
    pub cloud_mode: bool,
    /// Whether `require_bearer` is actually wired in for this run (i.e. a
    /// real `api_token` was configured) -- distinct from `cloud_mode`
    /// itself now that a token is optional in cloud mode. Reported to
    /// clients via `/api/status` so the UI doesn't assume auth is
    /// required just because it's talking to a cloud deployment.
    pub requires_authentication: bool,
}

#[derive(Clone)]
struct ApiToken(Arc<str>);

async fn require_bearer(
    State(expected): State<ApiToken>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let expected = format!("Bearer {}", expected.0);
            constant_time_eq(value.as_bytes(), expected.as_bytes())
        });
    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(types::ErrorResponse::new("missing or invalid bearer token")),
        )
            .into_response()
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

/// Cap rayon's global thread pool instead of trusting auto-detection.
///
/// `std::thread::available_parallelism()` (what rayon's default sizing uses)
/// reads the *host* machine's core count on many container runtimes, not
/// the container's actual cgroup CPU quota -- a well-known containerized-
/// Rust gotcha. Observed live on Railway: a real ~27k-function binary that
/// parses/decodes in under a second locally (14 real cores, no limits)
/// instead produced a request that never returned even after several
/// minutes -- consistent with rayon spinning up far more worker threads
/// than the container's actual CPU quota can run at once, so the whole
/// pool thrashes on scheduling/context-switch overhead instead of doing
/// useful work. Overridable via `FISSION_RAYON_THREADS` once the actual
/// allocated vCPU count for a given deployment is known; the default just
/// needs to be small enough to avoid catastrophic oversubscription on a
/// constrained container while still using a few real cores locally.
fn cap_rayon_thread_pool() {
    let threads = std::env::var("FISSION_RAYON_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8)
        });
    if let Err(e) = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
    {
        tracing::warn!("rayon global thread pool already initialized ({e}); FISSION_RAYON_THREADS cap not applied");
    } else {
        tracing::info!("rayon global thread pool capped at {threads} threads");
    }
}

/// Start the fission HTTP API server with the given configuration.
/// Blocks until the server is shut down.
pub async fn run_serve(config: ServeConfig) -> Result<()> {
    config.validate().map_err(anyhow::Error::msg)?;
    cap_rayon_thread_pool();
    let store = Arc::new(SessionStore::new(config.max_sessions, config.session_ttl_secs));

    // Start background TTL sweeper
    {
        let store = store.clone();
        tokio::spawn(async move {
            store.run_sweeper().await;
        });
    }

    let cors = build_cors(&config.allowed_origins);

    let protected = Router::new()
        .route("/api/status",                    get(handlers::status::handle_status))
        .route("/api/binary",                    post(handlers::binary::handle_upload_binary))
        .route("/api/functions/{session}",        get(handlers::functions::handle_list_functions))
        .route("/api/decompile/{session}/{addr}",  post(handlers::decompile::handle_decompile))
        .route("/api/xrefs/{session}/{addr}",      get(handlers::xrefs::handle_xrefs))
        .route("/api/session/{session}",          delete(handlers::binary::handle_delete_session));

    let protected = if let Some(token) = config.api_token.as_deref() {
        protected.route_layer(middleware::from_fn_with_state(
            ApiToken(Arc::from(token)),
            require_bearer,
        ))
    } else {
        protected
    };

    let app = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .merge(protected)
        .with_state(store)
        .layer(Extension(ServeRuntimeInfo {
            cloud_mode: config.cloud_mode,
            requires_authentication: config.api_token.is_some(),
        }))
        .layer(cors)
        .layer(DefaultBodyLimit::max(config.max_upload_bytes));

    let addr = SocketAddr::new(config.host.parse()?, config.port);
    info!("fission-serve  →  http://{}:{}", config.host, config.port);
    info!("Max sessions: {}  |  Session TTL: {}s  |  Upload limit: {}MB",
        config.max_sessions,
        config.session_ttl_secs,
        config.max_upload_bytes / 1024 / 1024,
    );
    info!(
        "Backend mode: {}  |  API authentication: {}",
        if config.cloud_mode { "cloud" } else { "local" },
        if config.api_token.is_some() { "bearer" } else { "disabled" },
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
