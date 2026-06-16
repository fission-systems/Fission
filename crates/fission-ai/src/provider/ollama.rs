//! Ollama local provider — calls the Ollama `/api/chat` streaming endpoint.

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use super::{AiProvider, ChunkStream, ProviderError, ProviderResult, ResponseChunk};
use crate::auth::ENV_FISSION_AI_OLLAMA_URL;
use crate::session::Message;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

// ── Ollama Models API types ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelObj>,
}

#[derive(Deserialize)]
struct OllamaModelObj {
    name: String,
}

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
        Self {
            client,
            base_url,
            model,
        }
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

    async fn fetch_models(&self) -> ProviderResult<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Ollama HTTP {status}: {body}"
            )));
        }

        let tags_resp: OllamaTagsResponse = resp.json().await?;
        let mut names: Vec<String> = tags_resp.models.into_iter().map(|m| m.name).collect();
        names.sort();
        Ok(names)
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _tools: Option<&[crate::tools::ToolDefinition]>,
    ) -> ProviderResult<ChunkStream> {
        use crate::session::Role;

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let ollama_msgs: Vec<OllamaMessage<'_>> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                },
                content: m.content.as_deref().unwrap_or_default(),
            })
            .collect();

        let resp = self
            .client
            .post(&url)
            .json(&OllamaChatReq {
                model: &self.model,
                messages: ollama_msgs,
                stream: true,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Ollama HTTP {status}: {body}"
            )));
        }

        let mut byte_stream = resp.bytes_stream();
        let chunk_stream = async_stream::stream! {
            let mut buffer = Vec::new();

            while let Some(item) = byte_stream.next().await {
                match item {
                    Err(e) => {
                        yield Err(ProviderError::Http(e));
                        return;
                    }
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);

                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes = &buffer[..pos];
                            let line = String::from_utf8_lossy(line_bytes).trim().to_string();
                            buffer = buffer[pos + 1..].to_vec();

                            if line.is_empty() {
                                continue;
                            }

                            match serde_json::from_str::<OllamaChunk>(&line) {
                                Ok(c) => {
                                    yield Ok(ResponseChunk {
                                        delta: c.message.content,
                                        tool_calls: None,
                                        done: c.done,
                                    });
                                }
                                Err(e) => {
                                    yield Err(ProviderError::Json(e));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(chunk_stream))
    }
}
