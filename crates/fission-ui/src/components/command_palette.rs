//! Command palette — opened via Cmd+K (or Ctrl+K on Windows/Linux).
//!
//! Design:
//!  - Full-screen semi-transparent backdrop.
//!  - Centred card with a prominent search input.
//!  - Results are fuzzy-scored and sorted in real-time.
//!  - Arrow keys move focus; Enter selects; Escape closes.
//!  - Clicking a result or the backdrop also dismisses.

use crate::state::{use_app_state, AppState, FunctionKind, LogEntry};
use dioxus::prelude::*;
use std::sync::Arc;

// ── SVG icons (palette-local) ────────────────────────────────────────────────

fn icon_fn() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "13", height: "13",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.8",
            stroke_linecap: "round",
            path { d: "M4 6h16M4 12h10M4 18h7" }
        }
    }
}

fn icon_import() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "11", height: "11",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            path { d: "M12 2v14M5 9l7 7 7-7" }
            path { d: "M5 22h14" }
        }
    }
}

fn icon_thunk() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "11", height: "11",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            path { d: "M5 12h14M14 6l6 6-6 6" }
        }
    }
}

// ── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn CommandPalette() -> Element {
    let mut state = use_app_state();

    // Short-circuit: render nothing when closed
    if !state.read().is_palette_open {
        return rsx! {};
    }

    let query = state.read().palette_query.clone();
    let focused = state.read().palette_focused;

    // Build fuzzy-ranked results — wrap in Arc so both the key-handler
    // closure and the render loop can own a reference without a move conflict.
    let results: std::sync::Arc<Vec<(i32, String, u64, bool, bool, Option<String>, Option<u64>)>> = {
        let s = state.read();
        let v = s
            .palette_results(18)
            .into_iter()
            .map(|(score, f)| {
                (
                    score,
                    f.name.clone(),
                    f.address,
                    f.is_import,
                    f.is_thunk_like,
                    f.external_library.clone(),
                    f.thunk_target,
                )
            })
            .collect();
        std::sync::Arc::new(v)
    };

    let result_count = results.len();

    // ── Keyboard handler (on the palette card) ───────────────────────────────
    let results_key = std::sync::Arc::clone(&results);
    let mut handle_key = move |e: Event<KeyboardData>| match e.key() {
        Key::Escape => {
            let mut s = state.write();
            s.is_palette_open = false;
            s.palette_query.clear();
            s.palette_focused = 0;
        }
        Key::ArrowDown => {
            let mut s = state.write();
            let max = result_count.saturating_sub(1);
            s.palette_focused = (s.palette_focused + 1).min(max);
        }
        Key::ArrowUp => {
            let mut s = state.write();
            s.palette_focused = s.palette_focused.saturating_sub(1);
        }
        Key::Enter => {
            if let Some((_, name, addr, is_import, is_thunk, lib, target)) =
                results_key.get(focused).cloned()
            {
                {
                    let mut s = state.write();
                    s.is_palette_open = false;
                    s.palette_query.clear();
                    s.palette_focused = 0;
                }
                trigger_decompile(state, addr, name, is_import, is_thunk, lib, target);
            }
        }
        _ => {}
    };

    rsx! {
        // ── Backdrop ─────────────────────────────────────────────────────────
        div {
            class: "palette-backdrop",
            onclick: move |_| {
                let mut s = state.write();
                s.is_palette_open = false;
                s.palette_query.clear();
                s.palette_focused = 0;
            },

            // ── Card (stop propagation so click doesn't close) ────────────
            div {
                class: "palette-card",
                onkeydown: handle_key,
                onclick: move |e| e.stop_propagation(),

                // Search input row
                div { class: "palette-search-row",
                    svg {
                        class: "palette-search-icon",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "16", height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                    }
                    input {
                        class: "palette-input",
                        r#type: "text",
                        placeholder: "Go to function…",
                        autofocus: true,
                        value: "{query}",
                        oninput: move |e| {
                            let mut s = state.write();
                            s.palette_query = e.value().clone();
                            s.palette_focused = 0;
                        },
                    }
                    div { class: "palette-kbd-hint",
                        span { class: "palette-kbd", "Esc" }
                    }
                }

                // Divider
                div { class: "palette-divider" }

                // Results list
                if results.is_empty() {
                    div { class: "palette-empty",
                        if query.is_empty() {
                            "Type to search functions"
                        } else {
                            "No matches found"
                        }
                    }
                } else {
                    div { class: "palette-results",
                        for (idx, (score, name, addr, is_import, is_thunk, lib, thunk_target)) in results.iter().enumerate() {
                            {
                                let is_focused = idx == focused;
                                let display = if name.is_empty() {
                                    format!("sub_{addr:x}")
                                } else {
                                    name.clone()
                                };
                                let lib_clone  = lib.clone();
                                let tt_clone   = *thunk_target;
                                let ii = *is_import;
                                let it = *is_thunk;
                                let name_click = name.clone();
                                let addr_val   = *addr;

                                rsx! {
                                    div {
                                        class: if is_focused { "palette-item is-focused" } else { "palette-item" },
                                        key: "{addr}",
                                        onclick: move |_| {
                                            {
                                                let mut s = state.write();
                                                s.is_palette_open = false;
                                                s.palette_query.clear();
                                                s.palette_focused = 0;
                                            }
                                            trigger_decompile(
                                                state,
                                                addr_val,
                                                name_click.clone(),
                                                ii, it,
                                                lib_clone.clone(),
                                                tt_clone,
                                            );
                                        },

                                        // Function icon
                                        div { class: "palette-item-icon",
                                            if *is_thunk {
                                                {icon_thunk()}
                                            } else if *is_import {
                                                {icon_import()}
                                            } else {
                                                {icon_fn()}
                                            }
                                        }

                                        // Name + address
                                        div { class: "palette-item-body",
                                            span { class: "palette-item-name", "{display}" }
                                            span { class: "palette-item-addr", "0x{addr:x}" }
                                        }

                                        // Kind badge
                                        if *is_thunk {
                                            span { class: "palette-badge badge-thunk", "THUNK" }
                                        } else if *is_import {
                                            span { class: "palette-badge badge-imp", "IMPORT" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer
                    div { class: "palette-footer",
                        span { class: "palette-footer-hint",
                            span { class: "palette-kbd", "↑ ↓" }
                            " navigate"
                        }
                        span { class: "palette-footer-hint",
                            span { class: "palette-kbd", "Enter" }
                            " select"
                        }
                        span { class: "palette-footer-count",
                            "{result_count} results"
                        }
                    }
                }
            }
        }
    }
}

// ── Decompile trigger (shared by click and Enter) ────────────────────────────

fn trigger_decompile(
    mut state: Signal<AppState>,
    addr: u64,
    name: String,
    is_import: bool,
    is_thunk: bool,
    library: Option<String>,
    thunk_target: Option<u64>,
) {
    let kind = if is_import && !is_thunk {
        FunctionKind::Import {
            library: library.clone(),
        }
    } else if is_thunk {
        FunctionKind::Thunk {
            target: thunk_target,
        }
    } else {
        FunctionKind::Code
    };

    {
        let mut s = state.write();
        s.current_function_addr = Some(addr);
        s.current_function_kind = kind.clone();
        s.decompiled_code = None;
        s.decompiled_nir = None;
        s.current_cfg = None;
    }

    match kind {
        FunctionKind::Import { library } => {
            let lib_str = library.as_deref().unwrap_or("unknown");
            let text = format!(
                "/* Import stub \u{2014} no decompilable body.\n\
                 *\n\
                 *  Symbol  : {name}\n\
                 *  Address : 0x{addr:016x}\n\
                 *  Library : {lib_str}\n\
                 */"
            );
            let mut s = state.write();
            s.decompiled_code = Some(text);
            s.push_log(LogEntry::info(format!("Import stub: {name}")));
        }
        FunctionKind::Thunk { .. } | FunctionKind::Code => {
            let binary = state.read().binary.clone();
            let is_thunk_kind = matches!(kind, FunctionKind::Thunk { .. });

            {
                let mut s = state.write();
                s.is_decompiling = true;
                if is_thunk_kind {
                    s.push_log(LogEntry::warn(format!(
                        "Decompiling import thunk: {name} @ 0x{addr:x}"
                    )));
                } else {
                    s.push_log(LogEntry::info(format!("Decompiling {name} @ 0x{addr:x}")));
                }
            }

            // Use the shared helper from sidebar so CFG is also stored
            let session = state.read().server_session_id.clone();
            spawn(crate::components::sidebar::run_decompile(
                state, binary, session, addr, name,
            ));
        }
    }
}
