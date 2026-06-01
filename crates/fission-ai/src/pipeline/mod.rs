//! AI pipeline: ties together auth resolution, provider selection, and session management.

use crate::auth::{OAuthOptions, ResolvedAuth, resolve_auth};
use crate::provider::{
    ProviderConfig, ProviderKind, SharedAiProvider, build_provider, provider_kind_from_env,
};
use crate::session::SessionContext;
use crate::provider::{ChunkStream, ProviderResult};

/// High-level AI pipeline entry point.
///
/// Resolves authentication, selects the appropriate provider, and manages
/// session state for multi-turn conversations.
pub struct AiPipeline {
    provider: SharedAiProvider,
    session: SessionContext,
}

impl AiPipeline {
    /// Build a pipeline, auto-resolving auth and provider from environment /
    /// stored token, with an optional override.
    pub async fn from_env(system_prompt: Option<String>) -> Result<Self, crate::auth::AuthError> {
        let opts = OAuthOptions::default();
        let auth = resolve_auth(&opts).await?;
        let kind = provider_kind_from_env();
        Self::build(kind, auth, system_prompt, None)
    }

    /// Build with an explicit provider kind and pre-resolved auth.
    pub fn build(
        kind: ProviderKind,
        auth: ResolvedAuth,
        system_prompt: Option<String>,
        model: Option<String>,
    ) -> Result<Self, crate::auth::AuthError> {
        let cfg = ProviderConfig {
            kind,
            bearer_token: auth.bearer_token().map(str::to_string),
            base_url: None,
            model,
        };
        let provider = build_provider(cfg);
        Ok(Self { provider, session: SessionContext::new(system_prompt) })
    }

    /// Send a user message and return a streaming chunk stream.
    pub async fn send(&mut self, user_msg: &str) -> ProviderResult<ChunkStream> {
        self.session.push_user(user_msg);
        let msgs = self.session.full_messages();
        self.provider.chat_stream(&msgs).await
    }

    /// Append the assistant's completed response to session history.
    pub fn record_assistant_response(&mut self, response: String) {
        self.session.push_assistant(response);
    }

    /// Clear session history (start a new conversation).
    pub fn new_session(&mut self) {
        self.session.clear();
    }

    /// Reference to the current session context.
    pub fn session(&self) -> &SessionContext {
        &self.session
    }

    /// Reference to the active provider.
    pub fn provider(&self) -> &dyn crate::provider::AiProvider {
        self.provider.as_ref()
    }

    /// Returns a human-readable label for the status bar.
    pub fn status_label(&self) -> String {
        format!("{}:{}", self.provider.name(), self.provider.model())
    }
}

/// Convenience: collect a full streaming response into a String, calling
/// `on_chunk` for each delta (e.g. to print incrementally).
pub async fn collect_stream<F>(
    stream: ChunkStream,
    mut on_chunk: F,
) -> ProviderResult<String>
where
    F: FnMut(&str),
{
    use futures::StreamExt;
    let mut out = String::new();
    futures::pin_mut!(stream);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if !chunk.delta.is_empty() {
            on_chunk(&chunk.delta);
            out.push_str(&chunk.delta);
        }
        if chunk.done {
            break;
        }
    }
    Ok(out)
}
