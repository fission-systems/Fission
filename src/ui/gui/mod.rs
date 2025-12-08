//! GUI Module
//! 
//! Contains the main application and reusable widgets for the egui-based interface.

mod app;
mod state;
mod messages;
mod menu;
mod status_bar;
mod panels;
mod widgets;
pub mod theme;

pub use app::FissionApp;
pub use state::AppState;
pub use messages::AsyncMessage;
pub use widgets::*;

