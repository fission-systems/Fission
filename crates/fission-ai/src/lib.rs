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
    use crate::auth::ResolvedAuth;
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

    #[test]
    fn test_context_truncation() {
        use crate::session::ContextManager;
        let cm = ContextManager::new(1000, 50);
        let long_output = "A".repeat(100);
        let processed = cm.process_tool_output("disasm", long_output);
        assert!(processed.contains("Truncated"));
        assert!(processed.len() > 50);
        assert!(processed.contains("disasm"));
    }

    #[test]
    fn test_context_compaction() {
        use crate::session::{ContextManager, Message};
        let cm = ContextManager::new(50, 50);
        
        let mut messages = vec![
            Message::system("System prompt"),
            Message::user("Hello 1"),
            Message::assistant("Hi 1"),
            Message::user("Hello 2"),
            Message::assistant("Hi 2"),
            Message::user("Hello 3"),
            Message::assistant("Hi 3"),
            Message::user("Hello 4"),
            Message::assistant("Hi 4"),
        ];
        
        let compacted = cm.compact_history(&mut messages);
        assert!(compacted);
        // Should keep system prompt (index 0) + compaction sentinel (index 1) + last 4 messages = 6
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0].content.as_deref(), Some("System prompt"));
        assert!(messages[1].content.as_ref().unwrap().contains("compacted"));
        assert_eq!(messages[2].content.as_deref(), Some("Hello 3")); // part of the last 4
    }

    #[test]
    fn test_reversing_focus_formatting() {
        use crate::session::ContextManager;
        let mut cm = ContextManager::new(1000, 50);
        cm.focus.active_function_addr = Some("0x140001000".to_string());
        cm.focus.active_function_name = Some("main".to_string());
        
        let prompt = cm.format_focus_prompt();
        assert!(prompt.contains("0x140001000"));
        assert!(prompt.contains("main"));
    }

    #[tokio::test]
    async fn test_apply_patch_tool_executes() {
        use crate::tools::execution::{AiTool, ApplyPatchTool};
        use std::fs;
        
        let path = std::env::temp_dir().join("fission_mock_binary_patch.exe");
        fs::write(&path, b"mock exe contents").unwrap();
        
        let tool = ApplyPatchTool;
        let args = serde_json::json!({
            "addr": "0x401000",
            "action": "rename_function",
            "value": "target_func"
        });
        
        let result = tool.execute(&args, Some(&path)).await.unwrap();
        assert!(result.contains("Successfully applied patch"));
        
        // Verify sidecar was created and has correct contents
        let sidecar_path = path.with_extension("fission.json");
        assert!(sidecar_path.exists());
        let sidecar_content = fs::read_to_string(&sidecar_path).unwrap();
        let project: serde_json::Value = serde_json::from_str(&sidecar_content).unwrap();
        assert_eq!(
            project["user_function_names"]["4198400"],
            serde_json::json!("target_func")
        );
        
        // Clean up
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(sidecar_path);
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

    #[test]
    fn test_extract_function_name() {
        let code = r#"
        // Some comments
        /* More comments */
        void my_fancy_func(int a, char* b) {
            return;
        }
        "#;
        let name = super::tools::execution::extract_function_name(code, 0x401000);
        assert_eq!(name, "my_fancy_func");

        // Pointer return type
        let code_ptr = r#"
        char* get_string_ptr() {
            return "hello";
        }
        "#;
        let name_ptr = super::tools::execution::extract_function_name(code_ptr, 0x401000);
        assert_eq!(name_ptr, "get_string_ptr");

        // Fallback
        let name_fail = super::tools::execution::extract_function_name("invalid code", 0x401000);
        assert_eq!(name_fail, "func_0x401000");
    }

    #[tokio::test]
    async fn test_annotate_and_search_memory_tools() {
        use crate::tools::execution::{AiTool, AnnotateFunctionTool, SearchMemoryTool};
        use std::fs;

        let path = std::env::temp_dir().join("fission_mock_binary_mem.exe");
        fs::write(&path, b"mock exe contents").unwrap();

        // 1. Write an annotation
        let ann_tool = AnnotateFunctionTool;
        let ann_args = serde_json::json!({
            "addr": "0x401000",
            "notes": "This function handles user authentication and decrypts the main key."
        });
        let result = ann_tool.execute(&ann_args, Some(&path)).await.unwrap();
        assert!(result.contains("Successfully saved analysis annotation"));

        // Verify sidecar
        let sidecar_path = path.with_extension("fission.json");
        assert!(sidecar_path.exists());
        let sidecar_content = fs::read_to_string(&sidecar_path).unwrap();
        let project: serde_json::Value = serde_json::from_str(&sidecar_content).unwrap();
        assert_eq!(
            project["annotations"]["4198400"],
            serde_json::json!("This function handles user authentication and decrypts the main key.")
        );

        // 2. Perform a search
        let search_tool = SearchMemoryTool;
        let search_args = serde_json::json!({
            "query": "authentication"
        });
        let search_result = search_tool.execute(&search_args, Some(&path)).await.unwrap();
        assert!(search_result.contains("Found 1 matches"));
        assert!(search_result.contains("0x401000"));
        assert!(search_result.contains("user authentication"));

        // Search failure query
        let search_args_fail = serde_json::json!({
            "query": "non_existent_pattern"
        });
        let search_result_fail = search_tool.execute(&search_args_fail, Some(&path)).await.unwrap();
        assert!(search_result_fail.contains("No matches found"));

        // Clean up
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(sidecar_path);
    }

    #[tokio::test]
    async fn test_search_decompilation_cache() {
        use crate::tools::execution::{AiTool, SearchMemoryTool};
        use std::fs;

        let path = std::env::temp_dir().join("fission_mock_binary_decomp_cache.exe");
        fs::write(&path, b"mock exe contents").unwrap();

        // Create a mock sidecar with decompilation cache
        let sidecar_path = path.with_extension("fission.json");
        let project = serde_json::json!({
            "binary_path": path.display().to_string(),
            "decompilation_cache": {
                "4198400": {
                    "name": "target_func",
                    "code": "void target_func() {\n    int key = 0xbeef;\n}",
                    "timestamp": 123456789
                }
            }
        });
        fs::write(&sidecar_path, serde_json::to_string_pretty(&project).unwrap()).unwrap();

        let search_tool = SearchMemoryTool;
        let search_args = serde_json::json!({
            "query": "0xbeef"
        });
        let result = search_tool.execute(&search_args, Some(&path)).await.unwrap();
        assert!(result.contains("Found 1 matches"));
        assert!(result.contains("target_func"));
        assert!(result.contains("0x401000"));
        assert!(result.contains("0xbeef"));

        // Clean up
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(sidecar_path);
    }

    #[derive(Debug)]
    struct MockProvider;

    #[async_trait::async_trait]
    impl crate::provider::AiProvider for MockProvider {
        fn name(&self) -> &str { "mock" }
        fn model(&self) -> &str { "mock-model" }
        async fn chat_stream(&self, _messages: &[crate::session::Message], _tools: Option<&[crate::tools::ToolDefinition]>) -> crate::provider::ProviderResult<crate::provider::ChunkStream> {
            Err(crate::provider::ProviderError::Other("Not implemented".to_string()))
        }
        async fn chat(&self, _messages: &[crate::session::Message], _tools: Option<&[crate::tools::ToolDefinition]>) -> crate::provider::ProviderResult<String> {
            Ok("# Consolidated Mock Report\n\n- Match found.\n- Code is consolidated.".to_string())
        }
    }

    #[tokio::test]
    async fn test_consolidate_analysis_report() {
        use crate::pipeline::AiPipeline;
        use crate::session::SessionContext;
        use crate::tools::registry::ToolRegistry;
        use crate::session::ContextManager;
        use std::fs;

        let path = std::env::temp_dir().join("fission_mock_binary_consolidate.exe");
        fs::write(&path, b"mock exe contents").unwrap();

        // Write a mock sidecar with some cache
        let sidecar_path = path.with_extension("fission.json");
        let project = serde_json::json!({
            "binary_path": path.display().to_string(),
            "decompilation_cache": {
                "4198400": {
                    "name": "target_func",
                    "code": "void target_func() {\n    int key = 0xbeef;\n}",
                    "timestamp": 123456789
                }
            },
            "annotations": {
                "4198400": "This is an important function."
            }
        });
        fs::write(&sidecar_path, serde_json::to_string_pretty(&project).unwrap()).unwrap();

        // Reconstruct pipeline with MockProvider
        let provider = std::sync::Arc::new(MockProvider);
        let pipeline = AiPipeline {
            provider,
            session: std::sync::Arc::new(std::sync::Mutex::new(SessionContext::new(None, Some(path.clone())))),
            tool_registry: std::sync::Arc::new(ToolRegistry::new()),
            context_manager: std::sync::Arc::new(std::sync::Mutex::new(ContextManager::new(1000, 50))),
        };

        let result = pipeline.consolidate_analysis_report().await.unwrap();
        assert!(result.is_some());
        let report_path = result.unwrap();
        assert!(report_path.exists());

        let content = fs::read_to_string(&report_path).unwrap();
        assert!(content.contains("# Consolidated Mock Report"));
        assert!(content.contains("- Code is consolidated."));

        // Clean up
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(sidecar_path);
        let _ = fs::remove_file(report_path);
    }
}
