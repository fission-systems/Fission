#![allow(non_snake_case)]
#![allow(unused)]

use dioxus::prelude::*;
use dioxus_html::HasFileData;
use tracing::Level;

mod components;
mod engine;
mod state;

use components::bottom_panel::BottomPanel;
use components::command_palette::CommandPalette;
use components::editor::Editor;
use components::sidebar::Sidebar;
use components::title_bar::TitleBar;
use state::{LogEntry, init_app_state};

const STYLE: Asset = asset!("assets/style.css");

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");

    let cfg = dioxus::desktop::Config::new()
        .with_window(
            dioxus::desktop::tao::window::WindowBuilder::new()
                .with_title("Fission")
                .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1440.0_f64, 900.0_f64))
                .with_min_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(900.0_f64, 600.0_f64)),
        )
        .with_custom_head(
            concat!(
                r#"<link rel="preconnect" href="https://fonts.googleapis.com">"#,
                r#"<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>"#,
                r#"<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">"#,
            )
            .to_string(),
        );

    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    init_app_state();
    let mut state = state::use_app_state();

    // ── Layout state (local to App) ──────────────────────────────────────────
    let mut sidebar_width      = use_signal(|| 264.0_f64);
    let mut is_sidebar_resize  = use_signal(|| false);
    let mut bottom_h           = use_signal(|| 180.0_f64);
    let mut is_bottom_resize   = use_signal(|| false);
    let mut bottom_resize_y    = use_signal(|| 0.0_f64);

    // ── Status bar derivations ───────────────────────────────────────────────
    let (indicator_cls, status_text) = {
        let s = state.read();
        if s.is_loading_binary   { ("status-indicator busy",     "Loading") }
        else if s.is_decompiling { ("status-indicator busy",     "Decompiling") }
        else if s.binary.is_some() { ("status-indicator ready",  "Ready") }
        else                     { ("status-indicator inactive", "Idle") }
    };

    let arch_seg = {
        let s = state.read();
        s.binary.as_ref().map(|b| format!(
            "{} \u{00B7} {}",
            if b.is_64bit { "x86-64" } else { "x86" },
            b.format,
        ))
    };

    let fn_seg = {
        let s = state.read();
        s.current_function_name().map(|name| {
            let addr = s.current_function_addr.unwrap_or(0);
            format!("{name}  @  0x{addr:x}")
        })
    };

    let sw              = *sidebar_width.read();
    let bh              = *bottom_h.read();
    let sidebar_visible = state.read().sidebar_visible;
    let panel_visible   = state.read().bottom_panel_visible;

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE }

        div {
            class: "app-container",
            tabindex: "0",
            autofocus: true,

            // ── Global keyboard shortcuts ────────────────────────────────────
            onkeydown: move |e| {
                let mods = e.modifiers();
                match e.key() {
                    // Cmd+K — command palette
                    Key::Character(s) if s == "k" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        state.write().is_palette_open = true;
                    }
                    // Cmd+B — toggle sidebar
                    Key::Character(s) if s == "b" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        state.write().toggle_sidebar();
                    }
                    // Cmd+J — toggle bottom panel
                    Key::Character(s) if s == "j" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        state.write().toggle_bottom_panel();
                    }
                    // Cmd+O — open binary
                    Key::Character(s) if s == "o" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        // Signal title_bar to open — use a flag in state
                        state.write().push_log(LogEntry::info("Use File \u{2192} Open Binary\u{2026} (Cmd O is handled by the menu)".to_string()));
                    }
                    // Escape — close palette
                    Key::Escape => {
                        let mut s = state.write();
                        if s.is_palette_open {
                            s.is_palette_open = false;
                            s.palette_query.clear();
                            s.palette_focused = 0;
                        }
                    }
                    _ => {}
                }
            },

            // ── Pointer handlers for drag-resize ─────────────────────────────
            onpointerup: move |_| {
                is_sidebar_resize.set(false);
                is_bottom_resize.set(false);
            },
            onpointerleave: move |_| {
                is_sidebar_resize.set(false);
                is_bottom_resize.set(false);
            },
            onpointermove: move |e| {
                if *is_sidebar_resize.read() {
                    let x = e.client_coordinates().x as f64;
                    *sidebar_width.write() = x.clamp(160.0, 520.0);
                }
                if *is_bottom_resize.read() {
                    let y      = e.client_coordinates().y as f64;
                    let prev_y = *bottom_resize_y.read();
                    let delta  = prev_y - y;  // drag upward → increases height
                    let new_h  = (*bottom_h.read() + delta).clamp(80.0, 480.0);
                    *bottom_h.write()          = new_h;
                    *bottom_resize_y.write()   = y;
                }
            },

            // ── Drag & Drop binary loading ────────────────────────────────────
            ondragover: move |e| { e.prevent_default(); },
            ondrop: move |e| {
                e.prevent_default();
                // Dioxus desktop exposes file paths through the drag event
                if let Some(files) = e.files() {
                    let paths = files.files();
                    if let Some(path_str) = paths.first() {
                        let path = std::path::PathBuf::from(path_str);
                        {
                            let mut s = state.write();
                            s.is_loading_binary = true;
                            s.push_log(LogEntry::info(format!("Loading {}", path.display())));
                        }
                        spawn(async move {
                            let result = tokio::task::spawn_blocking(move || {
                                engine::load_binary_blocking(&path)
                            }).await.unwrap_or_else(|e| Err(format!("Join error: {e}")));

                            match result {
                                Ok(load) => {
                                    let summary   = load.summary.clone();
                                    let fn_count  = load.functions.len();
                                    let mut s = state.write();
                                    s.loaded_binary_path   = Some(std::path::PathBuf::from(
                                        load.binary.format.clone()
                                    ));
                                    s.binary               = Some(std::sync::Arc::clone(&load.binary));
                                    s.functions            = load.functions;
                                    s.current_function_addr = None;
                                    s.decompiled_code      = None;
                                    s.decompiled_nir       = None;
                                    s.current_cfg          = None;
                                    s.sidebar_search       = String::new();
                                    s.is_loading_binary    = false;
                                    s.push_log(LogEntry::info(format!("Loaded \u{2014} {summary}")));
                                    s.push_log(LogEntry::info(format!("{fn_count} functions discovered.")));
                                }
                                Err(e) => {
                                    let mut s = state.write();
                                    s.is_loading_binary = false;
                                    s.push_log(LogEntry::error(format!("Load failed: {e}")));
                                }
                            }
                        });
                    }
                }
            },

            // Title bar
            TitleBar {}

            // ── Main workspace ───────────────────────────────────────────────
            div { class: "main-workspace",

                // Sidebar (conditionally rendered)
                if sidebar_visible {
                    div {
                        class: "sidebar-wrapper",
                        style: "width: {sw}px; min-width: {sw}px;",
                        Sidebar {}
                    }
                    // Sidebar resize handle
                    div {
                        class: if *is_sidebar_resize.read() { "resize-handle is-dragging" } else { "resize-handle" },
                        onpointerdown: move |e| {
                            e.prevent_default();
                            is_sidebar_resize.set(true);
                        },
                    }
                }

                // Editor + bottom panel column
                div { class: "editor-area",

                    // Code editor (fills remaining space)
                    div { class: "editor-region",
                        Editor {}
                    }

                    // Bottom panel resize handle
                    if panel_visible {
                        div {
                            class: if *is_bottom_resize.read() {
                                "bottom-resize-handle is-dragging"
                            } else {
                                "bottom-resize-handle"
                            },
                            onpointerdown: move |e| {
                                e.prevent_default();
                                is_bottom_resize.set(true);
                                *bottom_resize_y.write() = e.client_coordinates().y as f64;
                            },
                        }
                        div {
                            style: "height: {bh}px; min-height: {bh}px; max-height: {bh}px; overflow: hidden; display: flex; flex-direction: column;",
                            BottomPanel {}
                        }
                    }
                }
            }

            // ── Status bar ────────────────────────────────────────────────────
            div { class: "status-bar",
                div { class: "status-segment",
                    div { class: "{indicator_cls}" }
                    span { "{status_text}" }
                }
                if let Some(arch) = arch_seg {
                    div { class: "status-segment", "{arch}" }
                }
                if let Some(func) = fn_seg {
                    div { class: "status-segment", "{func}" }
                }
                if state.read().binary.is_some() {
                    div { class: "status-segment status-right",
                        "{state.read().functions.len()} functions"
                    }
                }
                // Toggle hints
                div { class: "status-segment status-right status-hint",
                    span { class: "status-kbd", "Cmd B" }
                    " Sidebar"
                    span { style: "margin-left:10px;", class: "status-kbd", "Cmd J" }
                    " Panel"
                    span { style: "margin-left:10px;", class: "status-kbd", "Cmd K" }
                    " Go to function"
                }
            }

            // Command palette (always on top)
            CommandPalette {}
        }
    }
}
