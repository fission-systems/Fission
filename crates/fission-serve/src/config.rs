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

    /// Run as an internet-facing backend with packaged resources.
    #[arg(long, default_value_t = false, env = "FISSION_CLOUD_MODE")]
    pub cloud_mode: bool,

    /// Bearer token required by cloud-mode API routes.
    #[arg(long, env = "FISSION_SERVE_API_TOKEN")]
    pub api_token: Option<String>,

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
            cloud_mode:       false,
            api_token:        None,
            allowed_origins:  vec![
                "http://localhost:3000".into(),
                "http://localhost:8080".into(),
                "http://localhost:7331".into(),
                "https://fission-system-web.vercel.app".into(),
            ],
        }
    }
}

impl ServeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.cloud_mode
            && self
                .api_token
                .as_deref()
                .is_none_or(|token| token.trim().len() < 32)
        {
            return Err(
                "cloud mode requires FISSION_SERVE_API_TOKEN with at least 32 characters"
                    .to_string(),
            );
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
    fn cloud_mode_rejects_missing_or_short_tokens() {
        let mut config = ServeConfig {
            cloud_mode: true,
            ..ServeConfig::default()
        };
        assert!(config.validate().is_err());
        config.api_token = Some("short".to_string());
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
