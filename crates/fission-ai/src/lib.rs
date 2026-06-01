//! Fission AI agent platform abstractions.

pub mod agent {
    /// High-level AI agent orchestration entrypoint.
    pub trait Agent {
        fn id(&self) -> &str;
    }
}

pub mod context {
    use serde::{Deserialize, Serialize};

    /// Minimal AI session context placeholder for future provider integration.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SessionContext {
        pub goal: Option<String>,
        pub inputs: Vec<String>,
    }
}

pub mod providers {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum ProviderError {
        #[error("provider is not configured")]
        NotConfigured,
    }

    pub trait Provider {
        fn name(&self) -> &str;
    }
}

pub mod suggestions {
    /// A lightweight suggestion type that AI integrations can surface to callers.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Suggestion {
        pub title: String,
        pub body: String,
    }
}

pub mod tasks {
    /// Placeholder task handle for future AI-assisted workflow execution.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TaskHandle {
        pub id: String,
    }
}

pub use fission_core as core;
