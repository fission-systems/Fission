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
use state::{init_app_state, LogEntry};

const STYLE: Asset = asset!("/assets/style.css");

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");

    let cfg = dioxus::desktop::Config::new()
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title("Fission")
                .with_inner_size(dioxus::desktop::LogicalSize::new(1440.0_f64, 900.0_f64))
                .with_min_inner_size(dioxus::desktop::LogicalSize::new(900.0_f64, 600.0_f64)),
        )
        .with_custom_head(
            format!(
                r#"<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">
<style>{}</style>"#,
                include_str!("../assets/style.css")
            )
        );

    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    init_app_state();
    let mut state = state::use_app_state();

    // ── Layout state (local to App) ──────────────────────────────────────────
    let mut sidebar_width = use_signal(|| 264.0_f64);
    let mut is_sidebar_resize = use_signal(|| false);
    let mut bottom_h = use_signal(|| 180.0_f64);
    let mut is_bottom_resize = use_signal(|| false);
    let mut bottom_resize_y = use_signal(|| 0.0_f64);

    // ── Status bar derivations ───────────────────────────────────────────────
    let (indicator_cls, status_text) = {
        let s = state.read();
        if s.is_loading_binary {
            ("status-indicator busy", "Loading")
        } else if s.is_decompiling {
            ("status-indicator busy", "Decompiling")
        } else if s.is_batch_running {
            ("status-indicator batch", "Analysing")
        } else if s.binary.is_some() {
            ("status-indicator ready", "Ready")
        } else {
            ("status-indicator inactive", "Idle")
        }
    };

    let arch_seg = {
        let s = state.read();
        s.binary.as_ref().map(|b| {
            format!(
                "{} \u{00B7} {}",
                if b.is_64bit { "x86-64" } else { "x86" },
                b.format,
            )
        })
    };

    let fn_seg = {
        let s = state.read();
        s.current_function_name().map(|name| {
            let addr = s.current_function_addr.unwrap_or(0);
            format!("{name}  @  0x{addr:x}")
        })
    };

    let sw = *sidebar_width.read();
    let bh = *bottom_h.read();
    let sidebar_visible = state.read().sidebar_visible;
    let panel_visible = state.read().bottom_panel_visible;

    let is_batch = state.read().is_batch_running;
    let batch_done = state.read().batch_done;
    let batch_total = state.read().batch_total;
    let batch_pct = if batch_total > 0 {
        batch_done * 100 / batch_total
    } else {
        0
    };

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
                    // Cmd+F — toggle find bar
                    Key::Character(s) if s == "f" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        let cur = state.read().find_bar_open;
                        state.write().find_bar_open = !cur;
                    }
                    // Cmd+O — open binary
                    Key::Character(s) if s == "o" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        state.write().push_log(LogEntry::info("Use File \u{2192} Open Binary\u{2026} (Cmd O is handled by the menu)".to_string()));
                    }
                    // Cmd+[ — navigate back
                    Key::Character(s) if s == "[" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        let addr = state.write().nav_back();
                        if let Some(addr) = addr {
                            let binary = state.read().binary.clone();
                            let name = state.read().functions.iter()
                                .find(|f| f.address == addr)
                                .map(|f| f.name.clone())
                                .unwrap_or_else(|| format!("sub_{addr:x}"));
                            if let Some(binary) = binary {
                                {
                                    let mut s = state.write();
                                    s.current_function_addr = Some(addr);
                                    s.decompiled_code = None;
                                    s.decompiled_nir  = None;
                                    s.current_cfg     = None;
                                    s.current_xref_callers.clear();
                                    s.current_xref_callees.clear();
                                    s.is_loading_xrefs = false;
                                    s.is_decompiling  = true;
                                    s.push_log(LogEntry::info(format!("Back -> {name}  @  0x{addr:x}")));
                                }
                                spawn(async move {
                                    engine::run_nav_decompile(state, Some(binary), None, addr, name).await;
                                });
                            }
                        }
                    }
                    // Cmd+] — navigate forward
                    Key::Character(s) if s == "]" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        let addr = state.write().nav_forward();
                        if let Some(addr) = addr {
                            let binary = state.read().binary.clone();
                            let name = state.read().functions.iter()
                                .find(|f| f.address == addr)
                                .map(|f| f.name.clone())
                                .unwrap_or_else(|| format!("sub_{addr:x}"));
                            if let Some(binary) = binary {
                                {
                                    let mut s = state.write();
                                    s.current_function_addr = Some(addr);
                                    s.decompiled_code = None;
                                    s.decompiled_nir  = None;
                                    s.current_cfg     = None;
                                    s.current_xref_callers.clear();
                                    s.current_xref_callees.clear();
                                    s.is_loading_xrefs = false;
                                    s.is_decompiling  = true;
                                    s.push_log(LogEntry::info(format!("Forward -> {name}  @  0x{addr:x}")));
                                }
                                spawn(async move {
                                    engine::run_nav_decompile(state, Some(binary), None, addr, name).await;
                                });
                            }
                        }
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
                // Dioxus 0.7: e.files() returns Vec<FileData> directly
                let file_list = e.files();
                if let Some(file_data) = file_list.first() {
                    let path = file_data.path();
                    {
                        let mut s = state.write();
                        s.is_loading_binary = true;
                        s.push_log(LogEntry::info(format!("Loading {}", path.display())));
                    }
                    spawn(async move {
                        let path_clone = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            engine::load_binary_blocking(&path_clone)
                        }).await.unwrap_or_else(|e| Err(format!("Join error: {e}")));
                        match result {
                            Ok(load) => {
                                let summary  = load.summary.clone();
                                let fn_count = load.functions.len();
                                let mut s = state.write();
                                s.binary_name           = Some(path.file_name().unwrap_or_default().to_string_lossy().into_owned());
                                s.binary                = load.binary.clone();
                                s.functions             = load.functions;
                                s.strings               = load.strings;
                                s.current_function_addr = None;
                                s.decompiled_code       = None;
                                s.decompiled_nir        = None;
                                s.current_cfg           = None;
                                s.sidebar_search        = String::new();
                                s.rename_map.clear();
                                s.is_loading_binary     = false;
                                s.push_log(LogEntry::info(format!("Loaded — {summary}")));
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
                // Batch progress
                if is_batch {
                    div { class: "status-segment batch-progress",
                        div { class: "batch-bar-outer",
                            div { class: "batch-bar-fill", style: "width: {batch_pct}%;" }
                        }
                        span { "{batch_done} / {batch_total}" }
                    }
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
