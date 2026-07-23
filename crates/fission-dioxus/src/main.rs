#![allow(non_snake_case)]
#![allow(unused)]

use dioxus::prelude::*;
use tracing::Level;

mod components;
mod engine;
mod state;

use components::bottom_panel::BottomPanel;
use components::command_palette::CommandPalette;
use components::editor::Editor;
use components::sidebar::Sidebar;
use components::title_bar::TitleBar;
use state::init_app_state;

const STYLE: Asset = asset!("assets/style.css");

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");

    let cfg = dioxus::desktop::Config::new()
        .with_window(
            dioxus::desktop::tao::window::WindowBuilder::new()
                .with_title("Fission")
                .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(
                    1440.0_f64,
                    900.0_f64,
                ))
                .with_min_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(
                    900.0_f64,
                    600.0_f64,
                )),
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

    // ── Sidebar drag-resize state ────────────────────────────────────────────
    // Kept local to App so sidebar width changes don't re-render the whole tree
    let mut sidebar_width = use_signal(|| 264.0_f64);
    let mut is_resizing   = use_signal(|| false);

    // ── Status bar derivations ───────────────────────────────────────────────
    let (indicator_cls, status_text) = {
        let s = state.read();
        if s.is_loading_binary       { ("status-indicator busy",     "Loading") }
        else if s.is_decompiling     { ("status-indicator busy",     "Decompiling") }
        else if s.binary.is_some()   { ("status-indicator ready",    "Ready") }
        else                         { ("status-indicator inactive", "Idle") }
    };

    let arch_seg = {
        let s = state.read();
        s.binary.as_ref().map(|b| format!(
            "{} · {}",
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

    let sw = *sidebar_width.read();

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE }

        // ── Root container — captures global hotkeys ─────────────────────────
        div {
            class: "app-container",
            tabindex: "0",
            autofocus: true,
            // Global keyboard shortcuts
            onkeydown: move |e| {
                let mods = e.modifiers();
                match e.key() {
                    // Cmd+K / Ctrl+K — open command palette
                    Key::Character(s) if s == "k" && (mods.meta() || mods.ctrl()) => {
                        e.prevent_default();
                        state.write().is_palette_open = true;
                    }
                    // Escape — close palette if open (when focus is on the root)
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
            // Stop resize drag when pointer leaves the app window
            onpointerup:    move |_| is_resizing.set(false),
            onpointerleave: move |_| is_resizing.set(false),
            onpointermove: move |e| {
                if *is_resizing.read() {
                    let x = e.client_coordinates().x as f64;
                    *sidebar_width.write() = x.clamp(160.0, 520.0);
                }
            },

            // Title bar
            TitleBar {}

            // Main workspace
            div { class: "main-workspace",
                // Sidebar with dynamic width
                div {
                    class: "sidebar-wrapper",
                    style: "width: {sw}px; min-width: {sw}px;",
                    Sidebar {}
                }

                // Drag handle between sidebar and editor
                div {
                    class: if *is_resizing.read() { "resize-handle is-dragging" } else { "resize-handle" },
                    onpointerdown: move |e| {
                        e.prevent_default();
                        is_resizing.set(true);
                    },
                }

                // Editor + bottom panel
                div { class: "editor-area",
                    Editor {}
                    BottomPanel {}
                }
            }

            // Status bar
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
                // Keyboard hint
                div { class: "status-segment status-right status-hint",
                    span { class: "status-kbd", "Cmd K" }
                    " Go to function"
                }
            }

            // Command palette — rendered above everything else
            CommandPalette {}
        }
    }
}
