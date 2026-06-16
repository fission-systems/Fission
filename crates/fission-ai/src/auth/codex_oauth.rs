//! Codex OAuth — Browser Authorization Code Flow with PKCE.
//!
//! Reference: vendor/opencode-1.15.13/packages/opencode/src/plugin/openai/codex.ts
//!
//! Flow:
//!   1. Generate PKCE verifier + challenge (SHA-256 / Base64-URL).
//!   2. Build authorize URL → open browser automatically.
//!   3. Spin up a single-shot local HTTP server on 127.0.0.1:1455.
//!   4. Wait for the OAuth redirect callback (code + state).
//!   5. Exchange code for tokens (access + refresh) via /oauth/token.
//!   6. Persist to TokenStore.
//!
//! Fallback (headless): `run_device_code_login` is kept for CI/SSH contexts.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::token_store::{StoredToken, TokenStore};
use super::{AuthError, AuthResult, OAuthOptions};

// ── ANSI colours ─────────────────────────────────────────────────────────────
const BLUE: &str = "\x1b[94m";
const GRAY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// ── Callback server ───────────────────────────────────────────────────────────
const OAUTH_PORT: u16 = 1455;
const CALLBACK_PATH: &str = "/auth/callback";
const CALLBACK_TIMEOUT_SECS: u64 = 300; // 5 minutes

// ── HTML pages ────────────────────────────────────────────────────────────────
const HTML_SUCCESS: &str = concat!(
    "<!doctype html><html><head><title>Fission AI — Authorized</title>",
    "<style>body{font-family:system-ui,sans-serif;display:flex;justify-content:center;",
    "align-items:center;height:100vh;margin:0;background:#0d1117;color:#e6edf3;}",
    ".box{text-align:center;padding:2rem;}h1{color:#58a6ff;}p{color:#8b949e;}</style></head>",
    "<body><div class='box'><h1>✓ Authorization Successful</h1>",
    "<p>You can close this window and return to Fission CLI.</p></div>",
    "<script>setTimeout(()=>window.close(),2000)</script></body></html>"
);

fn html_error(msg: &str) -> String {
    format!(
        concat!(
            "<!doctype html><html><head><title>Fission AI — Error</title>",
            "<style>body{{font-family:system-ui,sans-serif;display:flex;justify-content:center;",
            "align-items:center;height:100vh;margin:0;background:#0d1117;color:#e6edf3;}}",
            ".box{{text-align:center;padding:2rem;}}h1{{color:#f85149;}}p{{color:#8b949e;}}",
            ".err{{color:#ffa657;font-family:monospace;margin-top:1rem;padding:1rem;",
            "background:#161b22;border-radius:.5rem;}}</style></head>",
            "<body><div class='box'><h1>Authorization Failed</h1>",
            "<p>An error occurred.</p><div class='err'>{}</div></div></body></html>"
        ),
        msg
    )
}

// ── PKCE types ────────────────────────────────────────────────────────────────
struct PkceCodes {
    verifier: String,
    challenge: String,
}

fn generate_pkce() -> AuthResult<PkceCodes> {
    // 32 bytes of OS randomness → URL-safe Base64 verifier
    let mut raw = [0u8; 43];
    // Use std::collections::hash_map as simple entropy source — no external deps needed;
    // in production this is fine because OpenAI validates the verifier, not us.
    getrandom_simple(&mut raw)?;
    let verifier = URL_SAFE_NO_PAD.encode(raw);

    // SHA-256(verifier) → URL-safe Base64 challenge
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(hash.as_slice());

    Ok(PkceCodes {
        verifier,
        challenge,
    })
}

fn generate_state() -> AuthResult<String> {
    let mut raw = [0u8; 32];
    getrandom_simple(&mut raw)?;
    Ok(URL_SAFE_NO_PAD.encode(raw))
}

/// Portable entropy via /dev/urandom (macOS/Linux) or fallback.
fn getrandom_simple(buf: &mut [u8]) -> AuthResult<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|e| AuthError::Other(format!("failed to read random bytes: {e}")))?;
    Ok(())
}

// ── Token types ───────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}



// ── Browser helper ────────────────────────────────────────────────────────────
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

// ── Authorize URL ─────────────────────────────────────────────────────────────
fn build_authorize_url(opts: &OAuthOptions, pkce: &PkceCodes, state: &str) -> String {
    let issuer = opts.auth_base_url.trim_end_matches('/');
    let redirect_uri = format!("http://localhost:{OAUTH_PORT}{CALLBACK_PATH}");
    let params = [
        ("response_type", "code"),
        ("client_id", &opts.client_id),
        ("redirect_uri", &redirect_uri),
        ("scope", "openid profile email offline_access"),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("originator", "fission"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoded(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{issuer}/oauth/authorize?{query}")
}

fn urlencoded(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                vec![c]
            } else {
                format!("%{:02X}", c as u32).chars().collect()
            }
        })
        .collect()
}

// ── Single-shot local HTTP callback server ────────────────────────────────────

struct CallbackResult {
    code: String,
    state: String,
}

async fn wait_for_callback() -> AuthResult<CallbackResult> {
    let addr = format!("127.0.0.1:{OAUTH_PORT}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| AuthError::Other(format!("could not bind to {addr}: {e}")))?;

    let result = tokio::time::timeout(
        Duration::from_secs(CALLBACK_TIMEOUT_SECS),
        accept_one_request(&listener),
    )
    .await
    .map_err(|_| AuthError::Other("timed out waiting for browser callback (5 minutes)".into()))?;

    result
}

async fn accept_one_request(listener: &TcpListener) -> AuthResult<CallbackResult> {
    let (mut stream, _peer) = listener
        .accept()
        .await
        .map_err(|e| AuthError::Other(format!("accept error: {e}")))?;

    // Read the HTTP request (first 4 KiB is enough for the headers).
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| AuthError::Other(format!("read error: {e}")))?;
    let request = std::str::from_utf8(&buf[..n]).unwrap_or("");

    // Extract the request line: "GET /auth/callback?code=...&state=... HTTP/1.1"
    let request_line = request.lines().next().unwrap_or("");
    let path_part = request_line.split_whitespace().nth(1).unwrap_or("");

    // Parse query params
    let query = path_part.splitn(2, '?').nth(1).unwrap_or("");

    let mut code = None;
    let mut state = None;
    let mut error_desc = None;

    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let value = percent_decode(kv.next().unwrap_or(""));
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error_description" | "error" => error_desc = Some(value),
            _ => {}
        }
    }

    if let Some(desc) = error_desc {
        let body = html_error(&desc);
        send_html(&mut stream, 400, &body).await;
        return Err(AuthError::Other(format!("OAuth error: {desc}")));
    }

    match (code, state) {
        (Some(c), Some(s)) => {
            send_html(&mut stream, 200, HTML_SUCCESS).await;
            Ok(CallbackResult { code: c, state: s })
        }
        _ => {
            let body = html_error("Missing authorization code or state parameter.");
            send_html(&mut stream, 400, &body).await;
            Err(AuthError::Other("callback missing code or state".into()))
        }
    }
}

async fn send_html(stream: &mut tokio::net::TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ── Token exchange ────────────────────────────────────────────────────────────
async fn exchange_code_for_tokens(
    opts: &OAuthOptions,
    code: &str,
    pkce: &PkceCodes,
) -> AuthResult<TokenResponse> {
    let issuer = opts.auth_base_url.trim_end_matches('/');
    let token_url = format!("{issuer}/oauth/token");
    let redirect_uri = format!("http://localhost:{OAUTH_PORT}{CALLBACK_PATH}");

    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &opts.client_id),
        ("code_verifier", &pkce.verifier),
    ];

    let client = reqwest::Client::builder()
        .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(AuthError::Http)?;

    let resp = client
        .post(&token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await
        .map_err(AuthError::Http)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::TokenExchange(format!("HTTP {status}: {body}")));
    }

    resp.json::<TokenResponse>().await.map_err(AuthError::Http)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Full Browser OAuth + PKCE login. Opens browser automatically.
///
/// No manual opt-in setting required — works without enabling
/// "Device Code authentication" in ChatGPT security settings.
pub async fn run_browser_login(opts: &OAuthOptions) -> AuthResult<()> {
    let pkce = generate_pkce()?;
    let state = generate_state()?;
    let authorize_url = build_authorize_url(opts, &pkce, &state);

    println!("\n{GRAY}Fission AI — Codex OAuth login{RESET}");
    println!("\nOpening browser for authentication...");
    println!("{GRAY}If your browser does not open automatically, visit:{RESET}");
    println!("  {BLUE}{authorize_url}{RESET}\n");

    open_browser(&authorize_url);

    println!("{GRAY}Waiting for browser authorization (up to 5 minutes)...{RESET}");
    println!("{GRAY}Listening on http://127.0.0.1:{OAUTH_PORT}{CALLBACK_PATH}{RESET}\n");

    let callback = wait_for_callback().await?;

    // Validate state to prevent CSRF
    if callback.state != state {
        return Err(AuthError::Other(
            "OAuth state mismatch — possible CSRF attack, aborting".into(),
        ));
    }

    let tokens = exchange_code_for_tokens(opts, &callback.code, &pkce).await?;

    let expires_at = tokens.expires_in.map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + secs
    });

    TokenStore::save(
        &opts.fission_home,
        StoredToken {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at,
            provider: "codex".to_string(),
        },
    )
    .await?;

    println!("\n{BLUE}✓ Logged in successfully.{RESET} Token saved to ~/.fission/auth.json\n");
    Ok(())
}

// ── Device Code fallback (headless / SSH) ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval_secs: u64,
}

#[derive(Serialize)]
struct UserCodeReq<'a> {
    client_id: &'a str,
}

#[derive(Deserialize)]
struct UserCodeResp {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(default = "default_interval", deserialize_with = "de_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

fn de_interval<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("invalid interval")),
        serde_json::Value::String(s) => s.trim().parse::<u64>().map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom(
            "expected string or number for interval",
        )),
    }
}

#[derive(Serialize)]
struct TokenPollReq<'a> {
    device_auth_id: &'a str,
    user_code: &'a str,
}

#[derive(Deserialize)]
struct TokenPollOk {
    authorization_code: String,
    code_verifier: String,
}

/// Request a device code (headless/SSH fallback).
pub async fn request_device_code(opts: &OAuthOptions) -> AuthResult<DeviceCode> {
    let client = build_client()?;
    let base = opts.auth_base_url.trim_end_matches('/');
    let url = format!("{base}/api/accounts/deviceauth/usercode");

    let resp = client
        .post(&url)
        .json(&UserCodeReq {
            client_id: &opts.client_id,
        })
        .send()
        .await
        .map_err(|e| AuthError::DeviceCodeRequest(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(AuthError::DeviceCodeRequest(format!(
            "server returned {status}"
        )));
    }

    let body: UserCodeResp = resp
        .json()
        .await
        .map_err(|e| AuthError::DeviceCodeRequest(e.to_string()))?;

    Ok(DeviceCode {
        verification_url: format!("{base}/codex/device"),
        user_code: body.user_code,
        device_auth_id: body.device_auth_id,
        interval_secs: body.interval,
    })
}

pub fn print_device_code_prompt(code: &DeviceCode) {
    println!(
        "\n{GRAY}Fission AI — Codex OAuth login (headless){RESET}\n\
         \n1. Open this URL in your browser:\n   {BLUE}{url}{RESET}\
         \n\n2. Enter this one-time code {GRAY}(expires in 15 minutes){RESET}:\n   {BLUE}{code}{RESET}\
         \n\n{GRAY}Never share this code — device codes are a common phishing target.{RESET}\n",
        url = code.verification_url,
        code = code.user_code,
    );
}

pub async fn complete_device_code_login(opts: &OAuthOptions, code: &DeviceCode) -> AuthResult<()> {
    let client = build_client()?;
    let base = opts.auth_base_url.trim_end_matches('/');
    let poll_url = format!("{base}/api/accounts/deviceauth/token");
    let token_url = format!("{base}/oauth/token");
    let redirect_uri = format!("{base}/deviceauth/callback");

    let max_wait = std::time::Duration::from_secs(15 * 60);
    let start = std::time::Instant::now();
    let interval = std::time::Duration::from_secs(code.interval_secs.max(3));

    let poll_ok: TokenPollOk = loop {
        if start.elapsed() >= max_wait {
            return Err(AuthError::TokenPoll("timed out after 15 minutes".into()));
        }
        tokio::time::sleep(interval).await;

        let resp = client
            .post(&poll_url)
            .json(&TokenPollReq {
                device_auth_id: &code.device_auth_id,
                user_code: &code.user_code,
            })
            .send()
            .await
            .map_err(|e| AuthError::TokenPoll(e.to_string()))?;

        let status = resp.status();
        if status.is_success() {
            break resp
                .json()
                .await
                .map_err(|e| AuthError::TokenPoll(e.to_string()))?;
        }
        if !matches!(status.as_u16(), 403 | 404) {
            return Err(AuthError::TokenPoll(format!("unexpected status {status}")));
        }
    };

    let exchange_resp: TokenResponse = client
        .post(&token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &opts.client_id),
            ("code", &poll_ok.authorization_code),
            ("code_verifier", &poll_ok.code_verifier),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .map_err(AuthError::Http)?
        .json()
        .await
        .map_err(AuthError::Http)?;

    let expires_at = exchange_resp.expires_in.map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + secs
    });

    TokenStore::save(
        &opts.fission_home,
        StoredToken {
            access_token: exchange_resp.access_token,
            refresh_token: exchange_resp.refresh_token,
            expires_at,
            provider: "codex".to_string(),
        },
    )
    .await?;

    println!("\n{BLUE}✓ Logged in successfully.{RESET} Token saved to ~/.fission/auth.json\n");
    Ok(())
}

/// Convenience: full Device Code flow (headless/SSH contexts).
pub async fn run_device_code_login(opts: &OAuthOptions) -> AuthResult<()> {
    let code = request_device_code(opts).await?;
    print_device_code_prompt(&code);
    complete_device_code_login(opts, &code).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn build_client() -> AuthResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(AuthError::Http)
}
