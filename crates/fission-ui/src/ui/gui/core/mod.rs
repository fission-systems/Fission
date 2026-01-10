//! Core State Management Module
//!
//! This module contains the core state management components:
//! - Application state (AppState, AnalysisState, UIState)
//! - Domain models (pure business logic)
//! - ViewModels (UI-specific state)
//! - Async message passing (AsyncMessage)
//! - Command management (Command trait, CommandManager)

pub mod state;
pub mod domain;
pub mod viewmodels;
pub mod messages;
pub mod commands;

// Re-export commonly used types
pub use state::{AppState, AnalysisState, UIState, Activity};
pub use messages::AsyncMessage;
pub use commands::{Command, CommandManager};
