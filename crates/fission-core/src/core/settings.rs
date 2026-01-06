//! User settings persisted to disk.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

/// Settings and preferences state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SettingsState {
    /// UI Theme mode (Light/Dark/System)
    pub theme_mode: ThemeMode,
    /// UI Scale factor (0.5 to 2.0)
    pub ui_scale: f32,
    /// Show developer tools?
    pub show_dev_tools: bool,
    /// Code Editor font size
    pub editor_font_size: u32,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            ui_scale: 1.5,
            show_dev_tools: false,
            editor_font_size: 14,
        }
    }
}
