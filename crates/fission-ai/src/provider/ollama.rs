//! Ollama local provider — calls the Ollama `/api/chat` streaming endpoint.

use async_trait::async_trait;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use crate::session::Message;
use super::{AiProvider, ChunkStream, ProviderError, ProviderResult, ResponseChunk};
use crate::auth::ENV_FISSION_AI_OLLAMA_URL;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

// ── Ollama chat API types ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaChatReq<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OllamaChunk {
    message: OllamaChunkMsg,
    done: bool,
}

#[derive(Deserialize)]
struct OllamaChunkMsg {
    content: String,
}

// ── Provider ──────────────────────────────────────────────────────────────────

/// Ollama local model provider — no authentication required.
#[derive(Debug)]
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(base_url: Option<String>, model: String) -> Self {
        let base_url = base_url
            .or_else(|| std::env::var(ENV_FISSION_AI_OLLAMA_URL).ok())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        let client = reqwest::Client::builder()
            .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");
        Self { client, base_url, model }
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn requires_auth(&self) -> bool {
        false
    }

    async fn chat_stream(&self, messages: &[Message]) -> ProviderResult<ChunkStream> {
        use crate::session::Role;

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let ollama_msgs: Vec<OllamaMessage<'_>> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: &m.content,
            })
            .collect();

        let resp = self
            .client
            .post(&url)
            .json(&OllamaChatReq { model: &self.model, messages: ollama_msgs, stream: true })
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!("Ollama HTTP {status}: {body}")));
        }

        let byte_stream = resp.bytes_stream();
        let chunk_stream = byte_stream.flat_map(|item| {
            let results: Vec<ProviderResult<ResponseChunk>> = match item {
                Err(e) => vec![Err(ProviderError::Http(e))],
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    text.lines()
                        .filter(|l| !l.trim().is_empty())
                        .filter_map(|line| {
                            serde_json::from_str::<OllamaChunk>(line)
                                .ok()
                                .map(|c| Ok(ResponseChunk { delta: c.message.content, done: c.done }))
                        })
                        .collect()
                }
            };
            stream::iter(results)
        });

        Ok(Box::pin(chunk_stream))
    }
}
