//! Token persistence: reads and writes `~/.fission/auth.json`.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::AuthError;

const AUTH_FILE: &str = "auth.json";

/// Stored credential bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    /// Short-lived access token (Bearer).
    pub access_token: String,
    /// Long-lived refresh token used to obtain a new access token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) at which the access token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Provider identifier (e.g. "codex", "openai").
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_provider() -> String {
    "codex".to_string()
}

impl StoredToken {
    /// Returns `true` if the access token has not yet expired (or has no known expiry).
    pub fn is_valid(&self) -> bool {
        match self.expires_at {
            None => true,
            Some(exp) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Allow a 60-second grace period for clock skew.
                now + 60 < exp
            }
        }
    }
}

/// Thin wrapper around the on-disk auth file.
#[derive(Debug)]
pub struct TokenStore {
    path: PathBuf,
    token: Option<StoredToken>,
}

impl TokenStore {
    /// Load the token store from `<fission_home>/auth.json`.
    /// Returns `Err` if the file doesn't exist or cannot be parsed.
    pub async fn load(fission_home: &Path) -> Result<Self, AuthError> {
        let path = fission_home.join(AUTH_FILE);
        let bytes = fs::read(&path)
            .await
            .map_err(|e| AuthError::TokenStore(format!("read {}: {e}", path.display())))?;
        let token: StoredToken = serde_json::from_slice(&bytes)
            .map_err(|e| AuthError::TokenStore(format!("parse {}: {e}", path.display())))?;
        Ok(Self { path, token: Some(token) })
    }

    /// Save a token to `<fission_home>/auth.json`, creating the directory if needed.
    pub async fn save(fission_home: &Path, token: StoredToken) -> Result<(), AuthError> {
        fs::create_dir_all(fission_home)
            .await
            .map_err(|e| AuthError::TokenStore(format!("mkdir {}: {e}", fission_home.display())))?;
        let path = fission_home.join(AUTH_FILE);
        let bytes = serde_json::to_vec_pretty(&token)
            .map_err(|e| AuthError::TokenStore(format!("serialize: {e}")))?;
        fs::write(&path, &bytes)
            .await
            .map_err(|e| AuthError::TokenStore(format!("write {}: {e}", path.display())))?;
        tracing::debug!("saved auth token to {}", path.display());
        Ok(())
    }

    /// Remove the auth file (logout).
    pub async fn clear(fission_home: &Path) -> Result<(), AuthError> {
        let path = fission_home.join(AUTH_FILE);
        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| AuthError::TokenStore(format!("remove {}: {e}", path.display())))?;
        }
        Ok(())
    }

    /// Returns the access token string if stored and still valid.
    pub fn valid_access_token(&self) -> Option<&str> {
        self.token.as_ref().filter(|t| t.is_valid()).map(|t| t.access_token.as_str())
    }

    pub fn stored_token(&self) -> Option<&StoredToken> {
        self.token.as_ref()
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
