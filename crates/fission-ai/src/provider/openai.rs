//! OpenAI-compatible chat provider (used for both OpenAI and Codex backends).
//!
//! Handles Server-Sent Events (SSE) streaming via `reqwest` byte stream.

use async_trait::async_trait;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use crate::session::Message;
use crate::tools::ToolDefinition;
use super::{AiProvider, ChunkStream, ProviderError, ProviderResult, ResponseChunk, ProviderToolCallDelta, ProviderToolCallFunctionDelta};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

// ── Models API types ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelObj>,
}

#[derive(Deserialize)]
struct OpenAiModelObj {
    id: String,
}


// ── Request / response types ──────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatToolFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Serialize)]
struct ChatTool<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    function: ChatToolFunction<'a>,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ChatTool<'a>>>,
}

#[derive(Deserialize)]
struct SseToolCallFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct SseToolCallDelta {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<SseToolCallFunctionDelta>,
}

#[derive(Deserialize)]
struct SseDelta {
    content: Option<String>,
    tool_calls: Option<Vec<SseToolCallDelta>>,
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

/// OpenAI-compatible provider (OpenAI, Azure, vLLM, GitHub Copilot, …).
#[derive(Debug)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    bearer_token: Option<String>,
    base_url: String,
    model: String,
    /// Extra HTTP headers injected into every request (e.g. Copilot-specific headers).
    extra_headers: Vec<(String, String)>,
}

impl OpenAiProvider {
    pub fn new(bearer_token: Option<String>, base_url: Option<String>, model: String) -> Self {
        Self::new_with_extra_headers(bearer_token, base_url, model, vec![])
    }

    /// Construct with additional HTTP headers injected on every request.
    ///
    /// Used by [`super::copilot::CopilotProvider`] to attach Copilot-specific
    /// headers such as `Openai-Intent` and `x-initiator`.
    pub fn new_with_extra_headers(
        bearer_token: Option<String>,
        base_url: Option<String>,
        model: String,
        extra_headers: Vec<(String, String)>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("fission-ai/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            bearer_token,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model,
            extra_headers,
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

    async fn fetch_models(&self) -> ProviderResult<Vec<String>> {
        let token = self.bearer_token.as_deref().ok_or(ProviderError::NotAuthenticated)?;
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));

        let mut req = self.client.get(&url).bearer_auth(token);
        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {status}: {body}")));
        }

        let models_resp: OpenAiModelsResponse = resp.json().await?;
        let mut ids: Vec<String> = models_resp.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }

    async fn chat_stream(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> ProviderResult<ChunkStream> {
        let token = self.bearer_token.as_deref().ok_or(ProviderError::NotAuthenticated)?;

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        
        let chat_tools = tools.map(|ts| {
            ts.iter().map(|t| ChatTool {
                kind: "function",
                function: ChatToolFunction {
                    name: &t.callable_name,
                    description: &t.description,
                    parameters: &t.parameters,
                }
            }).collect::<Vec<_>>()
        });

        let mut req = self
            .client
            .post(&url)
            .bearer_auth(token)
            .json(&ChatRequest { model: &self.model, messages, stream: true, tools: chat_tools });

        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {status}: {body}")));
        }

        let mut byte_stream = resp.bytes_stream();
        
        let chunk_stream = async_stream::stream! {
            let mut buffer = String::new();
            
            while let Some(item) = byte_stream.next().await {
                match item {
                    Err(e) => {
                        yield Err(ProviderError::Http(e));
                        return;
                    }
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();
                            
                            if line.is_empty() { continue; }
                            
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    yield Ok(ResponseChunk { delta: String::new(), tool_calls: None, done: true });
                                    continue;
                                }
                                
                                match serde_json::from_str::<SseChunk>(data) {
                                    Ok(sse) => {
                                        for choice in sse.choices {
                                            let done = choice.finish_reason.as_deref() == Some("stop") || choice.finish_reason.as_deref() == Some("tool_calls");
                                            let delta = choice.delta.content.unwrap_or_default();
                                            let tool_calls = choice.delta.tool_calls.map(|tcs| {
                                                tcs.into_iter().map(|tc| ProviderToolCallDelta {
                                                    index: tc.index,
                                                    id: tc.id,
                                                    kind: tc.kind,
                                                    function: tc.function.map(|f| ProviderToolCallFunctionDelta {
                                                        name: f.name,
                                                        arguments: f.arguments,
                                                    }),
                                                }).collect()
                                            });
                                            
                                            if !delta.is_empty() || tool_calls.is_some() || done {
                                                yield Ok(ResponseChunk { delta, tool_calls, done });
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::debug!("SSE parse skip: {e} — data: {data}");
                                    }
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

