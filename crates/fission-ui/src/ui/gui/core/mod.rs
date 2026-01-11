//! Core State Management Module
//!
//! This module contains the core state management components:
//! - Application state (AppState, AnalysisState, UIState)
//! - Domain models (pure business logic)
//! - ViewModels (UI-specific state)
//! - Async message passing (AsyncMessage)
//! - Command management (Command trait, CommandManager)

pub mod commands;
pub mod domain;
pub mod messages;
pub mod state;
pub mod viewmodels;

// Re-export commonly used types
pub use commands::{Command, CommandManager};
pub use messages::AsyncMessage;
pub use state::{Activity, AnalysisState, AppState, UIState};
