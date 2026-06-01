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
    session: Arc<Mutex<SessionContext>>,
    pub tool_registry: Arc<ToolRegistry>,
    pub context_manager: Arc<Mutex<crate::session::ContextManager>>,
}

impl AiPipeline {
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
            
            // 2. Format dynamic focus prompt and combine it with the standard system prompt
            let focus_prompt = cm.format_focus_prompt();
            let mut base_prompt = session.system_prompt.clone().unwrap_or_else(|| {
                "You are Fission AI, a professional reverse engineering assistant.".to_string()
            });
            base_prompt.push_str(&focus_prompt);
            
            let mut msgs = Vec::new();
            msgs.push(crate::session::Message::system(base_prompt));
            msgs.extend(session.messages.iter().cloned());
            msgs
        };
        
        let tools = self.tool_registry.get_model_visible_tools();
        let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
        
        let provider = self.provider.clone();
        let mut inner_stream = provider.chat_stream(&msgs, tools_ref).await?;
        
        let session_clone = self.session.clone();
        let tool_registry_clone = self.tool_registry.clone();
        let context_manager_clone = self.context_manager.clone();
        
        // Return a wrapped stream that intercepts tool calls.
        let stream = async_stream::stream! {
            use futures::StreamExt;
            let mut full_delta = String::new();
            let mut pending_tool_calls: Vec<crate::provider::ProviderToolCallDelta> = Vec::new();

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

                        if !chunk.delta.is_empty() {
                            full_delta.push_str(&chunk.delta);
                            yield Ok(chunk);
                        }
                    }
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                }
            }

            // Stream ended, execute tools if any
            if !pending_tool_calls.is_empty() {
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
                    
                    let mut tool_result = String::new();
                    
                    yield Ok(crate::provider::ResponseChunk {
                        delta: format!("\n\n> [Tool] Calling `{}`(args: {})\n", func_name, func_args),
                        tool_calls: None,
                        done: false,
                    });
                    
                    if let Some(tool) = tool_registry_clone.get_tool(&func_name) {
                        if let Ok(args_json) = serde_json::from_str(&func_args) {
                            let binary_path = {
                                let session = session_clone.lock().unwrap();
                                session.binary_path.clone()
                            };
                            match tool.execute(&args_json, binary_path.as_deref()).await {
                                Ok(res) => tool_result = res,
                                Err(e) => tool_result = format!("Error executing tool: {}", e),
                            }
                        } else {
                            tool_result = format!("Error: Invalid JSON arguments: {}", func_args);
                        }
                    } else {
                        tool_result = format!("Error: Tool {} not found", func_name);
                    }
                    
                    // Process output through ContextManager (truncating if it exceeds size limits)
                    let processed_result = {
                        let cm = context_manager_clone.lock().unwrap();
                        cm.process_tool_output(&func_name, tool_result)
                    };
                    
                    // Update the active focus state based on tool arguments
                    {
                        let mut cm = context_manager_clone.lock().unwrap();
                        if func_name == "disasm" || func_name == "fission__disasm" {
                            if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(&func_args) {
                                if let Some(addr_val) = args_json.get("addr") {
                                    if let Some(addr_str) = addr_val.as_str() {
                                        cm.focus.active_function_addr = Some(addr_str.to_string());
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
                                        cm.focus.active_function_addr = Some(addr_str.to_string());
                                    }
                                }
                            }
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
                
                // Now restart the stream
                let new_msgs = {
                    let mut session = session_clone.lock().unwrap();
                    let cm = context_manager_clone.lock().unwrap();
                    
                    // Compact history on restarts if needed
                    cm.compact_history(&mut session.messages);
                    
                    let focus_prompt = cm.format_focus_prompt();
                    let mut base_prompt = session.system_prompt.clone().unwrap_or_else(|| {
                        "You are Fission AI, a professional reverse engineering assistant.".to_string()
                    });
                    base_prompt.push_str(&focus_prompt);
                    
                    let mut msgs = Vec::new();
                    msgs.push(crate::session::Message::system(base_prompt));
                    msgs.extend(session.messages.iter().cloned());
                    msgs
                };
                let tools2 = tool_registry_clone.get_model_visible_tools();
                let tools_ref2 = if tools2.is_empty() { None } else { Some(tools2.as_slice()) };
                match provider.chat_stream(&new_msgs, tools_ref2).await {
                    Ok(mut new_stream) => {
                        while let Some(chunk) = new_stream.next().await {
                            yield chunk;
                        }
                    }
                    Err(e) => yield Err(e),
                }
            } else {
                yield Ok(crate::provider::ResponseChunk { delta: String::new(), tool_calls: None, done: true });
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
