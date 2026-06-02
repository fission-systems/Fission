//! Session types shared between providers and the pipeline.

pub mod context_manager;
pub use context_manager::{ContextManager, ReversingFocus};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Chat message role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // usually "function"
    pub function: ToolCallFunction,
}

/// A single message in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    
    // For assistant messages calling tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    
    // For tool response messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: Some(content.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: Some(content.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: Some(content.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self { role: Role::Assistant, content: None, tool_calls: Some(tool_calls), tool_call_id: None, name: None }
    }
    pub fn tool_response(tool_call_id: String, name: String, content: String) -> Self {
        Self { role: Role::Tool, content: Some(content), tool_calls: None, tool_call_id: Some(tool_call_id), name: Some(name) }
    }
}

/// Agent specialization mode for system prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Analyst,
    Editor,
}

impl AgentMode {
    pub fn system_prompt_prefix(&self) -> &'static str {
        match self {
            AgentMode::Analyst => "You are Fission AI, a professional reverse engineering Analyst. Your goal is to explore, analyze, and explain code/binaries. You are read-only. You must not attempt to modify files. Provide deep insights.",
            AgentMode::Editor => "You are Fission AI, a professional reverse engineering Editor. Your goal is to modify binaries, write patches, or generate scripts. You have full write access. Focus on correct modifications.",
        }
    }
}

/// In-session conversation history.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    pub messages: Vec<Message>,
    pub system_prompt: Option<String>,
    pub binary_path: Option<PathBuf>,
    pub mode: AgentMode,
}

impl SessionContext {
    /// Create a new session with an optional system prompt and binary.
    pub fn new(system_prompt: Option<String>, binary_path: Option<PathBuf>) -> Self {
        Self { messages: Vec::new(), system_prompt, binary_path, mode: AgentMode::default() }
    }

    /// Returns the full message list including the system prompt prepended.
    pub fn full_messages(&self) -> Vec<Message> {
        let mut msgs = Vec::new();
        if let Some(sp) = &self.system_prompt {
            msgs.push(Message::system(sp));
        }
        msgs.extend(self.messages.iter().cloned());
        msgs
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(Message::assistant(content));
    }
    
    pub fn push_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
