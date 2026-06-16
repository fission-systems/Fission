//! Codex/ChatGPT provider — thin wrapper over [`OpenAiProvider`] with the
//! Codex base URL and Responses API wire format.

use async_trait::async_trait;

use super::openai::OpenAiProvider;
use super::{AiProvider, ChunkStream, ProviderResult};
use crate::session::Message;

const CODEX_BASE_URL: &str = "https://api.openai.com/v1";

/// Codex provider — uses ChatGPT OAuth bearer token.
#[derive(Debug)]
pub struct CodexProvider {
    inner: OpenAiProvider,
}

impl CodexProvider {
    pub fn new(bearer_token: Option<String>, model: String) -> Self {
        Self {
            inner: OpenAiProvider::new(bearer_token, Some(CODEX_BASE_URL.to_string()), model),
        }
    }
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn name(&self) -> &str {
        "codex"
    }

    fn model(&self) -> &str {
        self.inner.model()
    }

    fn requires_auth(&self) -> bool {
        true
    }

    async fn fetch_models(&self) -> ProviderResult<Vec<String>> {
        self.inner.fetch_models().await
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: Option<&[crate::tools::ToolDefinition]>,
    ) -> ProviderResult<ChunkStream> {
        self.inner.chat_stream(messages, tools).await
    }
}
