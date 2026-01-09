//! UI Components Module
//!
//! This module contains reusable UI components:
//! - Menu bar (top menu navigation)
//! - Status bar (bottom status display)
//! - Common widgets and UI helpers

pub mod menu;
pub mod status_bar;
pub mod widgets;

// Re-export commonly used types
pub use menu::{MenuAction, render as render_menu};
pub use status_bar::render as render_status_bar;
pub use widgets::*;
