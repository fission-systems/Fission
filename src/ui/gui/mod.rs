//! GUI Module
//! 
//! Contains the main application and reusable widgets for the egui-based interface.

mod app;
mod state;
mod messages;
mod menu;
pub mod commands;
mod status_bar;
mod panels;
mod widgets;
pub mod theme;

pub use app::FissionApp;
pub use state::{AppState, SettingsState, ThemeMode};
pub use messages::AsyncMessage;
pub use widgets::*;

