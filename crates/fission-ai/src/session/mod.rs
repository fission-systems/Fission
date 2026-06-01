//! Session types shared between providers and the pipeline.

use serde::{Deserialize, Serialize};

/// Chat message role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A single message in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// In-session conversation history.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    pub messages: Vec<Message>,
    pub system_prompt: Option<String>,
}

impl SessionContext {
    /// Create a new session with an optional system prompt.
    pub fn new(system_prompt: Option<String>) -> Self {
        Self { messages: Vec::new(), system_prompt }
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

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
