//! AI pipeline: ties together auth resolution, provider selection, and session management.

use crate::auth::{OAuthOptions, ResolvedAuth, resolve_auth};
use crate::provider::{
    ProviderConfig, ProviderKind, SharedAiProvider, build_provider, provider_kind_from_env,
};
use crate::session::SessionContext;
use crate::provider::{ChunkStream, ProviderResult};
use crate::tools::registry::ToolRegistry;

use std::sync::Arc;
use std::sync::Mutex;

/// High-level AI pipeline entry point.
///
/// Resolves authentication, selects the appropriate provider, and manages
/// session state for multi-turn conversations.
#[derive(Clone)]
pub struct AiPipeline {
    provider: SharedAiProvider,
    pub session: Arc<Mutex<SessionContext>>,
    pub tool_registry: Arc<ToolRegistry>,
    pub context_manager: Arc<Mutex<crate::session::ContextManager>>,
}

impl AiPipeline {
    pub fn set_agent_mode(&self, mode: crate::session::AgentMode) {
        self.session.lock().unwrap().mode = mode;
    }

    // ── Binary Context Bootstrap ───────────────────────────────────────────────

    /// Collect static binary facts (meta, symbols, strings) once when a binary is set.
    /// Runs blocking CLI calls inside `spawn_blocking` with a 3-second timeout.
    /// Never panics: on failure the snapshot stays `None` and the session starts normally.
    pub async fn init_binary_context(&self, binary_path: std::path::PathBuf) {
        let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("fission_cli"));
        let path_str = binary_path.display().to_string();

        let exe_clone = exe.clone();
        let path_clone = path_str.clone();
        let snapshot = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::task::spawn_blocking(move || {
                collect_binary_snapshot(&exe_clone, &path_clone)
            }),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();

        let mut cm = self.context_manager.lock().unwrap();
        cm.snapshot = snapshot;
    }

    /// Build a pipeline, auto-resolving auth and provider from environment /
    /// stored token, with an optional override.
    pub async fn from_env(system_prompt: Option<String>) -> Result<Self, crate::auth::AuthError> {
        let opts = OAuthOptions::default();
        let auth = resolve_auth(&opts).await?;
        let kind = provider_kind_from_env();
        Self::build(kind, auth, system_prompt, None, None)
    }

    /// Build with an explicit provider kind and pre-resolved auth.
    pub fn build(
        kind: ProviderKind,
        auth: ResolvedAuth,
        system_prompt: Option<String>,
        model: Option<String>,
        binary_path: Option<std::path::PathBuf>,
    ) -> Result<Self, crate::auth::AuthError> {
        let cfg = ProviderConfig {
            kind,
            bearer_token: auth.bearer_token().map(str::to_string),
            base_url: None,
            model,
        };
        let provider = build_provider(cfg);
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(crate::tools::execution::DisasmTool);
        tool_registry.register(crate::tools::execution::XrefsTool);
        tool_registry.register(crate::tools::execution::ApplyPatchTool);
        tool_registry.register(crate::tools::execution::LoadBinaryTool);
        
        let context_manager = crate::session::ContextManager::new(32000, 6000);
        
        Ok(Self {
            provider,
            session: Arc::new(Mutex::new(SessionContext::new(system_prompt, binary_path))),
            tool_registry: Arc::new(tool_registry),
            context_manager: Arc::new(Mutex::new(context_manager)),
        })
    }

    /// Send a user message and return a streaming chunk stream.
    pub async fn send(&self, user_msg: &str) -> ProviderResult<ChunkStream> {
        {
            let mut session = self.session.lock().unwrap();
            session.push_user(user_msg);
        }
        self.send_internal().await
    }
    
    pub async fn send_internal(&self) -> ProviderResult<ChunkStream> {
        let msgs = {
            let mut session = self.session.lock().unwrap();
            let cm = self.context_manager.lock().unwrap();
            
            // 1. Apply history compaction if message budget exceeded
            cm.compact_history(&mut session.messages);
            
            // 2. Build system prompt: AgentMode prefix + binary snapshot + focus state
            let mode = session.mode;
            let mut base_prompt = session.system_prompt.clone().unwrap_or_else(|| {
                mode.system_prompt_prefix().to_string()
            });
            base_prompt.push_str(&cm.format_binary_snapshot());
            base_prompt.push_str(&cm.format_focus_prompt());
            
            let mut msgs = Vec::new();
            msgs.push(crate::session::Message::system(base_prompt));
            msgs.extend(session.messages.iter().cloned());
            msgs
        };
        
        let session_clone = self.session.clone();
        let tool_registry_clone = self.tool_registry.clone();
        let context_manager_clone = self.context_manager.clone();
        let provider = self.provider.clone();
        
        // We will maintain local `current_msgs` for recursive tool calling loop
        let current_msgs = msgs;
        
        // Return a wrapped stream that intercepts tool calls.
        let stream = async_stream::stream! {
            use futures::StreamExt;
            let mut current_msgs = current_msgs;
            
            loop {
                let tools = tool_registry_clone.get_model_visible_tools();
                let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
                
                let mut inner_stream = match provider.chat_stream(&current_msgs, tools_ref).await {
                    Ok(s) => s,
                    Err(e) => { yield Err(e); return; }
                };

                let mut full_delta = String::new();
                let mut pending_tool_calls: Vec<crate::provider::ProviderToolCallDelta> = Vec::new();
                let mut yielded_done = false;

                while let Some(chunk_result) = inner_stream.next().await {
                    match chunk_result {
                        Ok(mut chunk) => {
                            // Aggregate tool calls
                            if let Some(tcs) = chunk.tool_calls.take() {
                                for tc in tcs {
                                    if let Some(existing) = pending_tool_calls.iter_mut().find(|t| t.index == tc.index) {
                                        if let Some(id) = tc.id { existing.id = Some(id); }
                                        if let Some(kind) = tc.kind { existing.kind = Some(kind); }
                                        if let Some(f) = tc.function {
                                            if let Some(ex_f) = existing.function.as_mut() {
                                                if let Some(name) = f.name { ex_f.name = Some(name); }
                                                if let Some(args) = f.arguments {
                                                    if let Some(ex_args) = ex_f.arguments.as_mut() {
                                                        ex_args.push_str(&args);
                                                    } else {
                                                        ex_f.arguments = Some(args);
                                                    }
                                                }
                                            } else {
                                                existing.function = Some(f);
                                            }
                                        }
                                    } else {
                                        pending_tool_calls.push(tc);
                                    }
                                }
                            }

                            // Do not yield `done: true` if we have pending tool calls
                            let mut to_yield = chunk;
                            if !pending_tool_calls.is_empty() {
                                to_yield.done = false;
                            } else {
                                yielded_done |= to_yield.done;
                            }

                            if !to_yield.delta.is_empty() || to_yield.done {
                                full_delta.push_str(&to_yield.delta);
                                yield Ok(to_yield);
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }

                // Stream ended
                if pending_tool_calls.is_empty() {
                    // No tools called, we are completely finished
                    if !yielded_done {
                        yield Ok(crate::provider::ResponseChunk { delta: String::new(), tool_calls: None, done: true });
                    }
                    break;
                }

                // Execute tools
                let mut session_tool_calls = Vec::new();
                
                for tc in &pending_tool_calls {
                    let id = tc.id.clone().unwrap_or_default();
                    let func_name = tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default();
                    let func_args = tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default();
                    
                    session_tool_calls.push(crate::session::ToolCall {
                        id: id.clone(),
                        kind: tc.kind.clone().unwrap_or_else(|| "function".to_string()),
                        function: crate::session::ToolCallFunction {
                            name: func_name.clone(),
                            arguments: func_args.clone(),
                        },
                    });
                }
                
                {
                    let mut session = session_clone.lock().unwrap();
                    session.push_message(crate::session::Message::assistant_tool_calls(session_tool_calls));
                }

                for tc in pending_tool_calls {
                    let id = tc.id.unwrap_or_default();
                    let func_name = tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default();
                    let func_args = tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default();
                    
                    yield Ok(crate::provider::ResponseChunk {
                        delta: format!("\n\n> [Tool] Calling `{}`(args: {})\n", func_name, func_args),
                        tool_calls: None,
                        done: false,
                    });
                    
                    let tool_result = if let Some(tool) = tool_registry_clone.get_tool(&func_name) {
                        if let Ok(args_json) = serde_json::from_str(&func_args) {
                            let binary_path = {
                                let session = session_clone.lock().unwrap();
                                session.binary_path.clone()
                            };
                            match tool.execute(&args_json, binary_path.as_deref()).await {
                                Ok(res) => res,
                                Err(e) => format!("Error executing tool: {}", e),
                            }
                        } else {
                            format!("Error: Invalid JSON arguments: {}", func_args)
                        }
                    } else {
                        format!("Error: Tool {} not found", func_name)
                    };
                    
                    let processed_result = {
                        let cm = context_manager_clone.lock().unwrap();
                        cm.process_tool_output(&func_name, tool_result)
                    };
                    
                    {
                        let mut cm = context_manager_clone.lock().unwrap();
                        if func_name == "load_binary" || func_name == "fission__load_binary" {
                            if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(&func_args) {
                                if let Some(path_val) = args_json.get("path") {
                                    if let Some(path_str) = path_val.as_str() {
                                        let path = std::path::PathBuf::from(path_str);
                                        if path.exists() && path.is_file() {
                                            {
                                                let mut session = session_clone.lock().unwrap();
                                                session.binary_path = Some(path.clone());
                                            }
                                            // Dynamically kick off snapshot collection
                                            let cm_bg = context_manager_clone.clone();
                                            let path_bg = path.clone();
                                            tokio::spawn(async move {
                                                let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("fission_cli"));
                                                let path_str_bg = path_bg.display().to_string();
                                                let snapshot = tokio::time::timeout(
                                                    std::time::Duration::from_secs(3),
                                                    tokio::task::spawn_blocking(move || {
                                                        collect_binary_snapshot(&exe, &path_str_bg)
                                                    }),
                                                )
                                                .await
                                                .ok()
                                                .and_then(|r| r.ok())
                                                .flatten();
                                                
                                                let mut cm = cm_bg.lock().unwrap();
                                                cm.snapshot = snapshot;
                                            });
                                        }
                                    }
                                }
                            }
                        } else if func_name == "disasm" || func_name == "fission__disasm" {
                            if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(&func_args) {
                                if let Some(addr_val) = args_json.get("addr") {
                                    if let Some(addr_str) = addr_val.as_str() {
                                        cm.focus.set_focus(addr_str.to_string(), None);
                                        let clean_addr = addr_str.trim_start_matches("0x").trim_start_matches("0X");
                                        if let Ok(start) = u64::from_str_radix(clean_addr, 16) {
                                            let count = args_json.get("count").and_then(|c| c.as_u64()).unwrap_or(20);
                                            cm.focus.last_disasm_range = Some((start, start + count * 4));
                                        }
                                    }
                                }
                            }
                        } else if func_name == "xrefs" || func_name == "fission__xrefs" {
                            if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(&func_args) {
                                if let Some(addr_val) = args_json.get("addr") {
                                    if let Some(addr_str) = addr_val.as_str() {
                                        cm.focus.set_focus(addr_str.to_string(), None);
                                        update_xrefs_from_output(&mut cm.focus, &processed_result);
                                    }
                                }
                            }
                        } else if func_name == "decomp" || func_name == "fission__decomp" {
                            cm.focus.set_decomp_snippet(processed_result.clone());
                        }
                    }
                    
                    yield Ok(crate::provider::ResponseChunk {
                        delta: format!("> [Tool] Result: {} bytes\n\n", processed_result.len()),
                        tool_calls: None,
                        done: false,
                    });
                    
                    {
                        let mut session = session_clone.lock().unwrap();
                        session.push_message(crate::session::Message::tool_response(id, func_name, processed_result));
                    }
                }
                
                // Now restart the stream by updating `current_msgs`
                current_msgs = {
                    let mut session = session_clone.lock().unwrap();
                    let cm = context_manager_clone.lock().unwrap();
                    
                    cm.compact_history(&mut session.messages);
                    
                    let mut base_prompt = session.system_prompt.clone().unwrap_or_else(|| {
                        "You are Fission AI, a professional reverse engineering assistant.".to_string()
                    });
                    base_prompt.push_str(&cm.format_binary_snapshot());
                    base_prompt.push_str(&cm.format_focus_prompt());
                    
                    let mut msgs = Vec::new();
                    msgs.push(crate::session::Message::system(base_prompt));
                    msgs.extend(session.messages.iter().cloned());
                    msgs
                };
            }
        };
        
        Ok(Box::pin(stream))
    }

    /// Append the assistant's completed response to session history.
    pub fn record_assistant_response(&self, response: String) {
        let mut session = self.session.lock().unwrap();
        session.push_assistant(response);
    }

    /// Clear session history (start a new conversation).
    pub fn new_session(&self) {
        let mut session = self.session.lock().unwrap();
        session.clear();
        let mut cm = self.context_manager.lock().unwrap();
        cm.focus = crate::session::ReversingFocus::default();
    }

    /// Dynamically swap the active provider.
    pub async fn switch_provider(&mut self, kind: ProviderKind) -> Result<(), crate::auth::AuthError> {
        let opts = crate::auth::OAuthOptions::default();
        let auth = crate::auth::resolve_auth(&opts).await?;
        let cfg = ProviderConfig {
            kind,
            bearer_token: auth.bearer_token().map(str::to_string),
            base_url: None,
            model: None, // Use provider's default model
        };
        self.provider = build_provider(cfg);
        Ok(())
    }

    /// Reference to the active provider.
    pub fn provider(&self) -> &dyn crate::provider::AiProvider {
        self.provider.as_ref()
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.provider.name().parse().unwrap_or(ProviderKind::Codex)
    }

    /// Fetch available models from the active provider.
    pub async fn fetch_models(&self) -> ProviderResult<Vec<String>> {
        self.provider.fetch_models().await
    }

    /// Dynamically swap the active model for the current provider.
    pub async fn switch_model(&mut self, model: String) -> Result<(), crate::auth::AuthError> {
        let opts = crate::auth::OAuthOptions::default();
        let auth = crate::auth::resolve_auth(&opts).await?;
        let cfg = ProviderConfig {
            kind: self.provider_kind(),
            bearer_token: auth.bearer_token().map(str::to_string),
            base_url: None, // Base URL isn't currently preserved across switches
            model: Some(model),
        };
        self.provider = build_provider(cfg);
        Ok(())
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

// ── Binary Snapshot Collection ─────────────────────────────────────────────────

/// Blocking: run CLI subcommands to collect binary metadata, function list, and strings.
/// Returns None on any failure (binary not found, CLI error, timeout propagated by caller).
fn collect_binary_snapshot(
    exe: &std::path::Path,
    binary_path: &str,
) -> Option<crate::session::context_manager::BinarySnapshot> {
    use std::process::Command;
    use crate::session::context_manager::BinarySnapshot;

    // 1. Binary metadata via `fission_cli info <binary>`
    let meta = Command::new(exe)
        .args(["info", binary_path])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    // If the binary doesn't exist / CLI fails to parse it, bail out early
    if meta.is_empty() {
        return None;
    }

    // Trim meta to a reasonable size
    let meta = if meta.len() > 1500 {
        format!("{}... [truncated]", &meta[..1500])
    } else {
        meta
    };

    // 2. Function list via `fission_cli list <binary>`
    let functions: Vec<String> = Command::new(exe)
        .args(["list", binary_path])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|out| {
            out.lines()
                .filter(|l| !l.trim().is_empty())
                .take(BinarySnapshot::MAX_FUNCTIONS)
                .map(|l| l.trim().to_string())
                .collect()
        })
        .unwrap_or_default();

    // 3. Strings via `fission_cli strings <binary> --min-len 6`
    let strings: Vec<String> = Command::new(exe)
        .args(["strings", binary_path, "--min-len", "6"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|out| {
            out.lines()
                .filter(|l| !l.trim().is_empty())
                .take(BinarySnapshot::MAX_STRINGS)
                .map(|l| l.trim().to_string())
                .collect()
        })
        .unwrap_or_default();

    Some(BinarySnapshot { meta, functions, strings })
}

/// Parse an xrefs tool output and update the focus with callers/callees.
/// This is best-effort: if the output can't be parsed we leave xrefs unchanged.
fn update_xrefs_from_output(
    focus: &mut crate::session::ReversingFocus,
    output: &str,
) {
    // The xrefs output is plain text with lines like:
    //   callers: <addr> <name>, <addr> <name>, ...
    //   callees: <addr> <name>, ...
    // Or JSON with `callers`/`callees` arrays.
    // We do a simple line-based heuristic.
    let mut callers = Vec::new();
    let mut callees = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("caller") {
            let rest = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            for entry in rest.split(',') {
                let s = entry.trim().to_string();
                if !s.is_empty() { callers.push(s); }
            }
        } else if trimmed.to_lowercase().starts_with("callee") || trimmed.to_lowercase().starts_with("call ") {
            let rest = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            for entry in rest.split(',') {
                let s = entry.trim().to_string();
                if !s.is_empty() { callees.push(s); }
            }
        }
    }

    if !callers.is_empty() { focus.xrefs_callers = callers; }
    if !callees.is_empty() { focus.xrefs_callees = callees; }
}
