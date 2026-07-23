//! Server configuration — all tunable parameters in one place.

use clap::Parser;

/// fission-serve configuration.
/// Can be supplied from CLI flags or environment variables.
#[derive(Debug, Clone, Parser)]
#[command(name = "fission-serve", about = "Fission multi-user decompilation API server")]
pub struct ServeConfig {
    /// IP address to bind (use 0.0.0.0 for all interfaces)
    #[arg(long, default_value = "127.0.0.1", env = "FISSION_SERVE_HOST")]
    pub host: String,

    /// TCP port to listen on
    #[arg(long, default_value_t = 7331, env = "PORT")]
    pub port: u16,

    /// Maximum number of concurrent sessions
    #[arg(long, default_value_t = 50, env = "FISSION_MAX_SESSIONS")]
    pub max_sessions: usize,

    /// Session time-to-live in seconds (inactive sessions are evicted)
    #[arg(long, default_value_t = 1800, env = "FISSION_SESSION_TTL")]
    pub session_ttl_secs: u64,

    /// Maximum binary upload size in bytes (default: 50 MiB)
    #[arg(long, default_value_t = 52_428_800, env = "FISSION_MAX_UPLOAD_BYTES")]
    pub max_upload_bytes: usize,

    /// CORS allowed origins (comma-separated, can repeat flag).
    /// Defaults include localhost ports for local development and
    /// the canonical Vercel deployment.
    #[arg(
        long,
        value_delimiter = ',',
        default_values = &[
            "http://localhost:3000",
            "http://localhost:8080",
            "http://localhost:7331",
            "https://fission-system-web.vercel.app",
        ],
        env = "FISSION_ALLOWED_ORIGINS"
    )]
    pub allowed_origins: Vec<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host:             "127.0.0.1".into(),
            port:             7331,
            max_sessions:     50,
            session_ttl_secs: 1800,
            max_upload_bytes: 52_428_800,
            allowed_origins:  vec![
                "http://localhost:3000".into(),
                "http://localhost:8080".into(),
                "http://localhost:7331".into(),
                "https://fission-system-web.vercel.app".into(),
            ],
        }
    }
}
