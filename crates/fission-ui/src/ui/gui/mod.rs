//! GUI Module
//!
//! Contains the main application and reusable widgets for the egui-based interface.

mod app;
pub mod commands;
mod menu;
mod messages;
mod panels;
mod state;
mod status_bar;
pub mod theme;
mod widgets;

pub use app::FissionApp;
pub use messages::AsyncMessage;
pub use fission_core::settings::{SettingsState, ThemeMode};
pub use state::AppState;
pub use widgets::*;
