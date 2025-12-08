//! Catppuccin-inspired theme for Fission GUI.
//!
//! Provides modern, eye-friendly color scheme with proper contrast.

use eframe::egui::{self, Color32, Rounding, Stroke, Vec2, FontFamily, FontId, TextStyle};
use std::collections::BTreeMap;

/// Catppuccin Mocha palette
pub mod catppuccin {
    use super::Color32;
    
    // Base colors
    pub const BASE: Color32 = Color32::from_rgb(30, 30, 46);      // #1e1e2e
    pub const MANTLE: Color32 = Color32::from_rgb(24, 24, 37);    // #181825
    pub const CRUST: Color32 = Color32::from_rgb(17, 17, 27);     // #11111b
    pub const SURFACE0: Color32 = Color32::from_rgb(49, 50, 68);  // #313244
    pub const SURFACE1: Color32 = Color32::from_rgb(69, 71, 90);  // #45475a
    pub const SURFACE2: Color32 = Color32::from_rgb(88, 91, 112); // #585b70
    
    // Text colors
    pub const TEXT: Color32 = Color32::from_rgb(205, 214, 244);   // #cdd6f4
    pub const SUBTEXT1: Color32 = Color32::from_rgb(186, 194, 222); // #bac2de
    pub const SUBTEXT0: Color32 = Color32::from_rgb(166, 173, 200); // #a6adc8
    pub const OVERLAY2: Color32 = Color32::from_rgb(147, 153, 178); // #9399b2
    pub const OVERLAY1: Color32 = Color32::from_rgb(127, 132, 156); // #7f849c
    pub const OVERLAY0: Color32 = Color32::from_rgb(108, 112, 134); // #6c7086
    
    // Accent colors
    pub const ROSEWATER: Color32 = Color32::from_rgb(245, 224, 220); // #f5e0dc
    pub const FLAMINGO: Color32 = Color32::from_rgb(242, 205, 205);  // #f2cdcd
    pub const PINK: Color32 = Color32::from_rgb(245, 194, 231);      // #f5c2e7
    pub const MAUVE: Color32 = Color32::from_rgb(203, 166, 247);     // #cba6f7
    pub const RED: Color32 = Color32::from_rgb(243, 139, 168);       // #f38ba8
    pub const MAROON: Color32 = Color32::from_rgb(235, 160, 172);    // #eba0ac
    pub const PEACH: Color32 = Color32::from_rgb(250, 179, 135);     // #fab387
    pub const YELLOW: Color32 = Color32::from_rgb(249, 226, 175);    // #f9e2af
    pub const GREEN: Color32 = Color32::from_rgb(166, 227, 161);     // #a6e3a1
    pub const TEAL: Color32 = Color32::from_rgb(148, 226, 213);      // #94e2d5
    pub const SKY: Color32 = Color32::from_rgb(137, 220, 235);       // #89dceb
    pub const SAPPHIRE: Color32 = Color32::from_rgb(116, 199, 236);  // #74c7ec
    pub const BLUE: Color32 = Color32::from_rgb(137, 180, 250);      // #89b4fa
    pub const LAVENDER: Color32 = Color32::from_rgb(180, 190, 254);  // #b4befe
}

/// Semantic colors for code highlighting
pub mod code {
    use super::catppuccin::*;
    
    pub const KEYWORD: super::Color32 = MAUVE;
    pub const FUNCTION: super::Color32 = BLUE;
    pub const STRING: super::Color32 = GREEN;
    pub const NUMBER: super::Color32 = PEACH;
    pub const COMMENT: super::Color32 = OVERLAY0;
    pub const OPERATOR: super::Color32 = SKY;
    pub const TYPE: super::Color32 = YELLOW;
    pub const REGISTER: super::Color32 = RED;
    pub const ADDRESS: super::Color32 = OVERLAY1;
    pub const MNEMONIC_FLOW: super::Color32 = RED;      // jmp, call, ret
    pub const MNEMONIC_NORMAL: super::Color32 = BLUE;   // mov, add, etc.
    pub const HEX_BYTE: super::Color32 = SUBTEXT0;
    pub const ASCII_PRINTABLE: super::Color32 = GREEN;
}

/// Apply Catppuccin theme to egui context
pub fn apply_catppuccin_theme(ctx: &egui::Context) {
    use catppuccin::*;
    
    let mut style = (*ctx.style()).clone();
    
    // Spacing and sizing
    style.spacing.item_spacing = Vec2::new(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.indent = 18.0;
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 10.0,
        bar_inner_margin: 4.0,
        bar_outer_margin: 0.0,
        ..Default::default()
    };
    
    // Rounding - modern rounded corners
    let rounding = Rounding::same(6.0);
    let small_rounding = Rounding::same(4.0);
    
    // Visuals
    let mut visuals = egui::Visuals::dark();
    
    // Window
    visuals.window_fill = BASE;
    visuals.window_stroke = Stroke::new(1.0, SURFACE0);
    visuals.window_rounding = rounding;
    visuals.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color: Color32::from_black_alpha(60),
    };
    
    // Panel
    visuals.panel_fill = BASE;
    
    // Widgets
    visuals.widgets.noninteractive.bg_fill = SURFACE0;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.rounding = small_rounding;
    visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    
    visuals.widgets.inactive.bg_fill = SURFACE0;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, SUBTEXT1);
    visuals.widgets.inactive.rounding = small_rounding;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    
    visuals.widgets.hovered.bg_fill = SURFACE1;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.rounding = small_rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BLUE);
    
    visuals.widgets.active.bg_fill = SURFACE2;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.rounding = small_rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, BLUE);
    
    visuals.widgets.open.bg_fill = SURFACE1;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.rounding = small_rounding;
    
    // Selection
    visuals.selection.bg_fill = BLUE.linear_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0, BLUE);
    
    // Hyperlink
    visuals.hyperlink_color = SAPPHIRE;
    
    // Faint background for striped tables
    visuals.faint_bg_color = SURFACE0.linear_multiply(0.5);
    
    // Extreme background
    visuals.extreme_bg_color = CRUST;
    
    // Code background
    visuals.code_bg_color = MANTLE;
    
    // Warn/error foreground
    visuals.warn_fg_color = YELLOW;
    visuals.error_fg_color = RED;
    
    // Text cursor
    visuals.text_cursor.width = 2.0;
    
    // Popup shadow
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 4.0),
        blur: 12.0,
        spread: 0.0,
        color: Color32::from_black_alpha(80),
    };
    
    // Resize corner
    visuals.resize_corner_size = 10.0;
    
    // Clip rect margin
    visuals.clip_rect_margin = 3.0;
    
    // Button frame
    visuals.button_frame = true;
    
    // Collapsing header frame
    visuals.collapsing_header_frame = true;
    
    // Indent has hover UI
    visuals.indent_has_left_vline = true;
    
    // Striped
    visuals.striped = true;
    
    // Slider trailing color
    visuals.slider_trailing_fill = true;
    
    style.visuals = visuals;
    
    ctx.set_style(style);
}

/// Configure custom fonts and text sizes
pub fn configure_fonts(ctx: &egui::Context) {
    // Configure font sizes for different text styles
    let mut style = (*ctx.style()).clone();
    
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ].into();
    
    ctx.set_style(style);
}

/// Load and apply JetBrains Mono font if available
pub fn load_jetbrains_mono(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Try to load JetBrains Mono from common locations
    #[cfg(target_os = "windows")]
    let font_paths = [
        "C:\\Windows\\Fonts\\JetBrainsMono-Regular.ttf",
        "C:\\Users\\Public\\Documents\\JetBrainsMono-Regular.ttf",
    ];
    
    #[cfg(not(target_os = "windows"))]
    let font_paths = [
        "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf",
    ];
    
    let mut font_loaded = false;
    for path in font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "JetBrainsMono".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            
            // Put JetBrainsMono first in monospace priority
            fonts.families
                .entry(FontFamily::Monospace)
                .or_default()
                .insert(0, "JetBrainsMono".to_owned());
            
            log::info!("Loaded JetBrains Mono from {}", path);
            font_loaded = true;
            break;
        }
    }
    
    if font_loaded {
        ctx.set_fonts(fonts);
    }
}

/// Initialize theme and fonts
pub fn init(ctx: &egui::Context) {
    apply_catppuccin_theme(ctx);
    configure_fonts(ctx);
    load_jetbrains_mono(ctx);
}

