//! Server configuration — all tunable parameters in one place.

use clap::Parser;

/// fission-serve configuration.
/// Can be supplied from CLI flags or environment variables.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "fission-serve",
    about = "Fission multi-user decompilation API server"
)]
pub struct ServeConfig {
    /// IP address to bind (use 0.0.0.0 for all interfaces)
    #[arg(long, default_value = "127.0.0.1", env = "FISSION_SERVE_HOST")]
    pub host: String,

    /// TCP port to listen on
    #[arg(long, default_value_t = 7331, env = "PORT")]
    pub port: u16,

    /// Maximum number of concurrent sessions. Each session holds a
    /// `LoadedBinary` + `FactStore` in process memory for its whole TTL, so
    /// this directly bounds worst-case memory. 10 matches the value
    /// `docs/RAILWAY.md` has long recommended via `FISSION_MAX_SESSIONS` for
    /// the single-replica cloud deployment -- moved here so a fresh deploy
    /// is safe-by-default without needing that env var set explicitly.
    #[arg(long, default_value_t = 10, env = "FISSION_MAX_SESSIONS")]
    pub max_sessions: usize,

    /// Session time-to-live in seconds (inactive sessions are evicted)
    #[arg(long, default_value_t = 1800, env = "FISSION_SESSION_TTL")]
    pub session_ttl_secs: u64,

    /// Maximum binary upload size in bytes (default: 50 MiB)
    #[arg(long, default_value_t = 52_428_800, env = "FISSION_MAX_UPLOAD_BYTES")]
    pub max_upload_bytes: usize,

    /// Run as an internet-facing backend with packaged resources.
    #[arg(long, default_value_t = false, env = "FISSION_CLOUD_MODE")]
    pub cloud_mode: bool,

    /// Optional bearer token for cloud-mode API routes. When unset, cloud
    /// mode relies on the deployment's own front door (e.g. an nginx
    /// `limit_req` gateway) for abuse protection instead of auth -- see
    /// `fission-web/deploy/nginx/default.conf.template` for the reference
    /// setup this option is meant to pair with.
    #[arg(long, env = "FISSION_SERVE_API_TOKEN")]
    pub api_token: Option<String>,

    /// CORS allowed origins (comma-separated, can repeat flag).
    /// Defaults include localhost ports for local development. Production uses
    /// the same-origin Railway gateway and does not require browser CORS.
    #[arg(
        long,
        value_delimiter = ',',
        default_values = &[
            "http://localhost:3000",
            "http://localhost:8080",
            "http://localhost:7331",
        ],
        env = "FISSION_ALLOWED_ORIGINS"
    )]
    pub allowed_origins: Vec<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 7331,
            max_sessions: 10,
            session_ttl_secs: 1800,
            max_upload_bytes: 52_428_800,
            cloud_mode: false,
            api_token: None,
            allowed_origins: vec![
                "http://localhost:3000".into(),
                "http://localhost:8080".into(),
                "http://localhost:7331".into(),
            ],
        }
    }
}

impl ServeConfig {
    pub fn validate(&self) -> Result<(), String> {
        // Cloud mode no longer *requires* a token -- a deployment can rely
        // on a front-door rate limiter instead (see `api_token`'s own doc).
        // A token, if supplied, still has to be a real secret, not a short
        // placeholder someone forgot to fill in.
        if let Some(token) = self.api_token.as_deref() {
            if token.trim().len() < 32 {
                return Err(
                    "FISSION_SERVE_API_TOKEN, if set, must be at least 32 characters".to_string(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mode_does_not_require_a_token() {
        assert!(ServeConfig::default().validate().is_ok());
    }

    #[test]
    fn cloud_mode_without_a_token_relies_on_the_front_door_rate_limiter() {
        let config = ServeConfig {
            cloud_mode: true,
            ..ServeConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn cloud_mode_rejects_a_short_token_if_one_is_set() {
        let config = ServeConfig {
            cloud_mode: true,
            api_token: Some("short".to_string()),
            ..ServeConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn cloud_mode_accepts_a_long_token() {
        let config = ServeConfig {
            cloud_mode: true,
            api_token: Some("0123456789abcdef0123456789abcdef".to_string()),
            ..ServeConfig::default()
        };
        assert!(config.validate().is_ok());
    }
}
