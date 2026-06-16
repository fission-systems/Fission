//! GitHub Copilot OAuth — Device Code Flow (RFC 8628).
//!
//! Reference: vendor/opencode-1.15.13/packages/opencode/src/plugin/github-copilot/copilot.ts
//!
//! Flow:
//!   1. POST https://github.com/login/device/code  → device_code, user_code, verification_uri, interval
//!   2. Print user_code + open browser to verification_uri.
//!   3. Poll https://github.com/login/oauth/access_token until approved.
//!   4. Store access_token as the Copilot refresh/access token.
//!   5. All API calls go to https://api.githubcopilot.com with the token.
//!
//! GitHub's Device Code flow does NOT require any special opt-in setting —
//! it follows standard RFC 8628 and is open to all GitHub Copilot subscribers.
//!
//! Refresh: Copilot tokens don't expire in the traditional sense; the token is
//! used directly as a Bearer token for the Copilot API.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::token_store::{StoredToken, TokenStore};
use super::{AuthError, AuthResult};

// ── ANSI colours ─────────────────────────────────────────────────────────────
const BLUE: &str = "\x1b[94m";
const GRAY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// ── Constants (from OpenCode reference) ──────────────────────────────────────
/// GitHub OAuth App Client ID used by OpenCode for Copilot.
pub const COPILOT_CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// GitHub Copilot API base URL.
pub const COPILOT_API_BASE: &str = "https://api.githubcopilot.com";
/// Polling safety buffer to avoid hitting GitHub slightly too early.
const POLLING_SAFETY_MS: u64 = 3_000;
/// Max time to wait for the user to complete browser auth.
const MAX_WAIT_SECS: u64 = 15 * 60;

// ── Device Code request/response ──────────────────────────────────────────────
#[derive(Serialize)]
struct DeviceCodeReq<'a> {
    client_id: &'a str,
    scope: &'a str,
}

#[derive(Deserialize, Debug)]
struct DeviceCodeResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<u64>,
}

// ── Token poll request/response ───────────────────────────────────────────────
#[derive(Serialize)]
struct TokenPollReq<'a> {
    client_id: &'a str,
    device_code: &'a str,
    grant_type: &'a str,
}

#[derive(Deserialize, Debug)]
struct TokenPollResp {
    access_token: Option<String>,
    error: Option<String>,
    interval: Option<u64>,
}

// ── Browser helper (same as codex_oauth) ─────────────────────────────────────
fn open_browser(url: &str) {
    let result = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(url).spawn()
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(url).spawn()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/c", "start", url])
                .spawn()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported platform",
            ))
        }
    };
    if let Err(e) = result {
        eprintln!("{GRAY}(could not open browser automatically: {e}){RESET}");
    }
}

fn build_client() -> AuthResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(AuthError::Http)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run the full GitHub Copilot Device Code login flow.
///
/// - Opens browser automatically to `https://github.com/login/device`.
/// - No special GitHub security setting required.
/// - Requires a GitHub Copilot Individual/Business/Enterprise subscription.
pub async fn run_copilot_login(fission_home: &std::path::Path) -> AuthResult<()> {
    let client = build_client()?;

    // ── Step 1: Request device code ───────────────────────────────────────────
    let resp = client
        .post(DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .json(&DeviceCodeReq {
            client_id: COPILOT_CLIENT_ID,
            scope: "read:user",
        })
        .send()
        .await
        .map_err(|e| AuthError::DeviceCodeRequest(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AuthError::DeviceCodeRequest(format!(
            "GitHub returned HTTP {}",
            resp.status()
        )));
    }

    let device: DeviceCodeResp = resp
        .json()
        .await
        .map_err(|e| AuthError::DeviceCodeRequest(e.to_string()))?;

    let poll_interval_ms = device.interval.unwrap_or(5) * 1_000 + POLLING_SAFETY_MS;

    // ── Step 2: Show prompt + open browser ───────────────────────────────────
    println!("\n{GRAY}Fission AI — GitHub Copilot login{RESET}");
    println!(
        "\n1. Opening browser: {BLUE}{url}{RESET}",
        url = device.verification_uri
    );
    println!(
        "2. Enter this one-time code:\n\n   {BLUE}{code}{RESET}\n",
        code = device.user_code
    );

    open_browser(&device.verification_uri);

    println!("{GRAY}Waiting for GitHub authorization (up to 15 minutes)...{RESET}\n");

    // ── Step 3: Poll for token ────────────────────────────────────────────────
    let start = std::time::Instant::now();

    let access_token = loop {
        if start.elapsed() > Duration::from_secs(MAX_WAIT_SECS) {
            return Err(AuthError::TokenPoll("timed out after 15 minutes".into()));
        }

        tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;

        let poll_resp = client
            .post(ACCESS_TOKEN_URL)
            .header("Accept", "application/json")
            .json(&TokenPollReq {
                client_id: COPILOT_CLIENT_ID,
                device_code: &device.device_code,
                grant_type: "urn:ietf:params:oauth:grant-type:device_code",
            })
            .send()
            .await
            .map_err(|e| AuthError::TokenPoll(e.to_string()))?;

        if !poll_resp.status().is_success() {
            return Err(AuthError::TokenPoll(format!(
                "unexpected status {}",
                poll_resp.status()
            )));
        }

        let data: TokenPollResp = poll_resp
            .json()
            .await
            .map_err(|e| AuthError::TokenPoll(e.to_string()))?;

        if let Some(token) = data.access_token {
            break token;
        }

        match data.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                // RFC 8628 §3.5: add 5 s to interval on slow_down
                let extra = data.interval.unwrap_or(5) * 1_000;
                tokio::time::sleep(Duration::from_millis(extra)).await;
                continue;
            }
            Some(other) => {
                return Err(AuthError::TokenPoll(format!("GitHub error: {other}")));
            }
            None => continue,
        }
    };

    // ── Step 4: Persist token ─────────────────────────────────────────────────
    // Copilot tokens don't have a standard expires_in — treat as long-lived.
    TokenStore::save(
        fission_home,
        StoredToken {
            access_token: access_token.clone(),
            refresh_token: Some(access_token), // GitHub uses the same token for refresh
            expires_at: None,                  // Copilot tokens are long-lived
            provider: "copilot".to_string(),
        },
    )
    .await?;

    println!("\n{BLUE}✓ Logged in to GitHub Copilot.{RESET} Token saved to ~/.fission/auth.json\n");
    Ok(())
}
