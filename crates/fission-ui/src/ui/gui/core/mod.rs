//! Core State Management Module
//!
//! This module contains the core state management components:
//! - Application state (AppState, AnalysisState, UIState)
//! - Async message passing (AsyncMessage)
//! - Command management (Command trait, CommandManager)

pub mod state;
pub mod messages;
pub mod commands;

// Re-export commonly used types
pub use state::{AppState, AnalysisState, UIState, Activity};
pub use messages::AsyncMessage;
pub use commands::{Command, CommandManager};
