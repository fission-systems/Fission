use dioxus::prelude::*;
use std::path::PathBuf;

/// Application state containing references to the engine.
/// We use Dioxus signals to automatically re-render components on change.
#[derive(Clone, Default)]
pub struct AppState {
    pub loaded_binary_path: Option<PathBuf>,
    pub current_function_addr: Option<u64>,
}

pub fn use_app_state() -> Signal<AppState> {
    use_context::<Signal<AppState>>()
}

pub fn init_app_state() {
    provide_context(Signal::new(AppState::default()));
}
