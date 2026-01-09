//! GUI Module
//!
//! Contains the main application and reusable widgets for the egui-based interface.
//!
//! # Module Structure
//!
//! - `app/` - Application logic and business operations
//! - `panels/` - UI panels (decompile view, assembly, functions, etc.)
//! - `core/` - Core state management and messaging
//! - `components/` - Reusable UI components (menu, status bar, widgets)
//! - `theme/` - Theme and styling system

mod app;
pub mod core;
pub mod components;
mod panels;
pub mod theme;

// Re-export commonly used types
pub use app::FissionApp;
pub use core::{AppState, AsyncMessage, Command, CommandManager};
pub use components::{MenuAction};
pub use fission_core::settings::{SettingsState, ThemeMode};
