//! OpenAI-compatible chat provider (used for both OpenAI and Codex backends).
//!
//! Handles Server-Sent Events (SSE) streaming via `reqwest` byte stream.

use async_trait::async_trait;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use crate::session::Message;
use super::{AiProvider, ChunkStream, ProviderError, ProviderResult, ResponseChunk};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
}

#[derive(Deserialize)]
struct SseDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct SseChoice {
    delta: SseDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}

// ── Provider ──────────────────────────────────────────────────────────────────

/// OpenAI-compatible provider (OpenAI, Azure, vLLM, …).
#[derive(Debug)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    bearer_token: Option<String>,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(bearer_token: Option<String>, base_url: Option<String>, model: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            bearer_token,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model,
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn chat_stream(&self, messages: &[Message]) -> ProviderResult<ChunkStream> {
        let token = self.bearer_token.as_deref().ok_or(ProviderError::NotAuthenticated)?;

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .json(&ChatRequest { model: &self.model, messages, stream: true })
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {status}: {body}")));
        }

        let byte_stream = resp.bytes_stream();
        let chunk_stream = byte_stream
            .flat_map(|item| {
                let result: Vec<ProviderResult<ResponseChunk>> = match item {
                    Err(e) => vec![Err(ProviderError::Http(e))],
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        parse_sse_lines(&text)
                    }
                };
                stream::iter(result)
            });

        Ok(Box::pin(chunk_stream))
    }
}

// ── SSE parsing ───────────────────────────────────────────────────────────────

fn parse_sse_lines(text: &str) -> Vec<ProviderResult<ResponseChunk>> {
    let mut chunks = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                chunks.push(Ok(ResponseChunk { delta: String::new(), done: true }));
                continue;
            }
            match serde_json::from_str::<SseChunk>(data) {
                Ok(sse) => {
                    for choice in sse.choices {
                        let done = choice.finish_reason.as_deref() == Some("stop");
                        let delta = choice.delta.content.unwrap_or_default();
                        if !delta.is_empty() || done {
                            chunks.push(Ok(ResponseChunk { delta, done }));
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("SSE parse skip: {e} — data: {data}");
                }
            }
        }
    }
    chunks
}
