//! Fission AI agent platform.
//!
//! Provides a multi-provider AI pipeline with:
//! - Codex/ChatGPT OAuth (Device Code Flow) — no API key required
//! - OpenAI-compatible API key backend
//! - Local Ollama backend
//!
//! # Quick start
//!
//! ```no_run
//! use fission_ai::pipeline::AiPipeline;
//! use fission_ai::pipeline::collect_stream;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut pipeline = AiPipeline::from_env(None).await.unwrap();
//!     let stream = pipeline.send("Summarize this binary's imports").await.unwrap();
//!     let response = collect_stream(stream, |chunk| print!("{chunk}")).await.unwrap();
//!     pipeline.record_assistant_response(response);
//! }
//! ```

pub mod auth;
pub mod pipeline;
pub mod provider;
pub mod session;
pub mod tools;

pub use pipeline::AiPipeline;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{OAuthOptions, ResolvedAuth};
    use crate::provider::{ProviderKind, ProviderConfig, build_provider};
    use crate::session::SessionContext;

    // ── ProviderKind parsing ──────────────────────────────────────────────────

    #[test]
    fn provider_kind_from_str_codex() {
        assert_eq!("codex".parse::<ProviderKind>().unwrap(), ProviderKind::Codex);
        assert_eq!("chatgpt".parse::<ProviderKind>().unwrap(), ProviderKind::Codex);
    }

    #[test]
    fn provider_kind_from_str_openai() {
        assert_eq!("openai".parse::<ProviderKind>().unwrap(), ProviderKind::OpenAi);
    }

    #[test]
    fn provider_kind_from_str_ollama() {
        assert_eq!("ollama".parse::<ProviderKind>().unwrap(), ProviderKind::Ollama);
    }

    #[test]
    fn provider_kind_from_str_unknown_errors() {
        assert!("unknown_provider".parse::<ProviderKind>().is_err());
    }

    #[test]
    fn provider_kind_display() {
        assert_eq!(ProviderKind::Codex.to_string(), "codex");
        assert_eq!(ProviderKind::OpenAi.to_string(), "openai");
        assert_eq!(ProviderKind::Ollama.to_string(), "ollama");
    }

    // ── ResolvedAuth ──────────────────────────────────────────────────────────

    #[test]
    fn resolved_auth_none_not_authenticated() {
        assert!(!ResolvedAuth::None.is_authenticated());
    }

    #[test]
    fn resolved_auth_api_key_authenticated() {
        assert!(ResolvedAuth::ApiKey("sk-test".to_string()).is_authenticated());
    }

    #[test]
    fn resolved_auth_none_bearer_is_none() {
        assert!(ResolvedAuth::None.bearer_token().is_none());
    }

    #[test]
    fn resolved_auth_api_key_bearer() {
        let auth = ResolvedAuth::ApiKey("sk-hello".to_string());
        assert_eq!(auth.bearer_token(), Some("sk-hello"));
    }

    // ── Session history ───────────────────────────────────────────────────────

    #[test]
    fn session_roundtrip() {
        let mut session = SessionContext::new(Some("system prompt".to_string()), None);
        session.push_user("hello");
        session.push_assistant("world".to_string());
        let msgs = session.full_messages();
        // system + user + assistant = 3
        assert_eq!(msgs.len(), 3);
        assert_eq!(format!("{:?}", msgs[0].role).to_lowercase(), "system");
        assert_eq!(format!("{:?}", msgs[1].role).to_lowercase(), "user");
        assert_eq!(format!("{:?}", msgs[2].role).to_lowercase(), "assistant");
    }

    #[test]
    fn session_clear_resets_history() {
        let mut session = SessionContext::new(None, None);
        session.push_user("hello");
        session.clear();
        let msgs = session.full_messages();
        assert!(msgs.is_empty());
    }

    // ── Build provider (smoke) ────────────────────────────────────────────────

    #[test]
    fn build_ollama_provider_no_auth() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Ollama,
            bearer_token: None,
            base_url: None,
            model: None,
        };
        let provider = build_provider(cfg);
        assert_eq!(provider.name(), "ollama");
        assert!(!provider.requires_auth());
    }

    #[test]
    fn build_codex_provider_label() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Codex,
            bearer_token: Some("tok".to_string()),
            base_url: None,
            model: Some("gpt-4o".to_string()),
        };
        let provider = build_provider(cfg);
        assert_eq!(provider.name(), "codex");
        assert_eq!(provider.model(), "gpt-4o");
    }
}
