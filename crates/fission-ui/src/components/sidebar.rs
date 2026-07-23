//! Sidebar — function list with:
//!   • `use_memo` for O(1) re-render on unrelated state changes
//!   • Virtual scroll — only renders the visible window of items
//!   • Import / Thunk classification before decompile

use crate::engine::DecompileOutput;
use crate::state::{use_app_state, AppState, FunctionKind, LogEntry};
use dioxus::prelude::*;
use std::sync::Arc;

// ── Layout constants ─────────────────────────────────────────────────────────
const ITEM_H: f64 = 32.0; // must match .function-item CSS height
const VISIBLE_H: f64 = 680.0; // pessimistic list-area height
const OVERSCAN: usize = 4; // extra items rendered above/below viewport

// ── SVGs ─────────────────────────────────────────────────────────────────────
fn svg_search() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "13", height: "13",
            view_box: "0 0 16 16",
            fill: "none", stroke: "currentColor",
            stroke_width: "1.5", stroke_linecap: "round",
            circle { cx: "7", cy: "7", r: "5" }
            line { x1: "11", y1: "11", x2: "14.5", y2: "14.5" }
        }
    }
}

fn svg_binary() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "22", height: "22",
            view_box: "0 0 24 24",
            fill: "none", stroke: "currentColor",
            stroke_width: "1.5", stroke_linecap: "round",
            path { d: "M6 4v16M10 4v16M4 8h4M4 16h4M14 4h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-2zM14 14h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-2z" }
        }
    }
}

// ── Entry snapshot ───────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
struct FnEntry {
    addr: u64,
    name: String,
    is_imp: bool,
    is_exp: bool,
    is_thunk: bool,
    lib: Option<String>,
    thunk_t: Option<u64>,
}

// ── Component ─────────────────────────────────────────────────────────────────
#[component]
pub fn Sidebar() -> Element {
    let mut state = use_app_state();

    // ── Memoised filter — recomputes only when functions or search changes ────
    let filtered: Memo<Vec<FnEntry>> = use_memo(move || {
        let s = state.read();
        let q = s.sidebar_search.to_lowercase();
        s.functions
            .iter()
            .filter(|f| {
                q.is_empty()
                    || f.name.to_lowercase().contains(&q)
                    || format!("{:x}", f.address).contains(&q)
            })
            .map(|f| FnEntry {
                addr: f.address,
                name: f.name.clone(),
                is_imp: f.is_import,
                is_exp: f.is_export,
                is_thunk: f.is_thunk_like,
                lib: f.external_library.clone(),
                thunk_t: f.thunk_target,
            })
            .collect()
    });

    // ── Virtual scroll state ─────────────────────────────────────────────────
    let mut scroll_top: Signal<f64> = use_signal(|| 0.0);

    // When sidebar_scroll_target changes, adjust virtual scroll position so
    // the target item is within the visible window.
    use_effect(move || {
        let target = state.read().sidebar_scroll_target;
        let Some(addr) = target else {
            return;
        };
        // Find the index of the target in the filtered list
        let idx = filtered.read().iter().position(|e| e.addr == addr);
        if let Some(idx) = idx {
            // Scroll so the item is roughly centred in the visible area
            let desired_top = (idx as f64 * ITEM_H - VISIBLE_H / 2.0 + ITEM_H / 2.0).max(0.0);
            *scroll_top.write() = desired_top;
        }
        // Acknowledge the scroll request
        state.write().sidebar_scroll_target = None;
    });

    // Derive visible slice from scroll position
    let total = filtered.read().len();
    let start = ((*scroll_top.read() / ITEM_H) as usize).saturating_sub(OVERSCAN);
    let visible = (VISIBLE_H / ITEM_H).ceil() as usize + OVERSCAN * 2;
    let end = (start + visible).min(total);

    let total_h = total as f64 * ITEM_H;
    let offset_top = start as f64 * ITEM_H;
    let spacer_bot = if end < total {
        (total - end) as f64 * ITEM_H
    } else {
        0.0
    };

    // ── Decompile on click ───────────────────────────────────────────────────
    let mut on_select = move |entry: FnEntry| {
        let kind = if entry.is_imp && !entry.is_thunk {
            FunctionKind::Import {
                library: entry.lib.clone(),
            }
        } else if entry.is_thunk {
            FunctionKind::Thunk {
                target: entry.thunk_t,
            }
        } else {
            FunctionKind::Code
        };

        {
            let mut s = state.write();
            s.current_function_addr = Some(entry.addr);
            s.current_function_kind = kind.clone();
            s.decompiled_code = None;
            s.decompiled_nir = None;
            s.current_cfg = None;
            s.current_xref_callers.clear();
            s.current_xref_callees.clear();
            s.is_loading_xrefs = false;
            s.navigate_to(entry.addr); // push to nav history
        }

        match kind {
            FunctionKind::Import { library } => {
                let lib_str = library.as_deref().unwrap_or("unknown");
                let stub = format!(
                    "/* Import stub — no decompilable body.\n\
                     *\n\
                     *  Symbol  : {}\n\
                     *  Address : 0x{:016x}\n\
                     *  Library : {lib_str}\n\
                     */",
                    entry.name, entry.addr,
                );
                let mut s = state.write();
                s.decompiled_code = Some(stub);
                s.push_log(LogEntry::info(format!(
                    "Import stub: {}  (from {lib_str})",
                    entry.name
                )));
            }

            FunctionKind::Thunk { target } => {
                let binary = state.read().binary.clone();
                let name = entry.name.clone();
                let addr = entry.addr;
                let tgt = target.map(|t| format!("0x{t:x}")).unwrap_or_default();
                {
                    let mut s = state.write();
                    s.is_decompiling = true;
                    s.push_log(LogEntry::warn(format!(
                        "\"{name}\" is an import thunk — output will appear self-recursive \
                         (IAT target: {tgt})"
                    )));
                }
                spawn(async move { run_decompile(state, binary, addr, name).await });
            }

            FunctionKind::Code => {
                let binary = state.read().binary.clone();
                let name = entry.name.clone();
                let addr = entry.addr;
                {
                    let mut s = state.write();
                    s.is_decompiling = true;
                    s.push_log(LogEntry::info(format!("Decompiling {name}  @  0x{addr:x}")));
                }
                spawn(async move { run_decompile(state, binary, addr, name).await });
            }
        }
    };

    // ── Read once for rendering ───────────────────────────────────────────────
    let has_binary = state.read().binary.is_some();
    let is_loading = state.read().is_loading_binary;
    let fn_count = total;
    let search_val = state.read().sidebar_search.clone();
    let selected = state.read().current_function_addr;

    rsx! {
        div { class: "sidebar",
            // Header
            div { class: "sidebar-header",
                span { class: "sidebar-title", "Functions" }
                if has_binary { span { class: "fn-badge", "{fn_count}" } }
            }

            // Search
            if has_binary {
                div { class: "sidebar-search",
                    div { class: "search-wrap",
                        span { class: "search-icon", {svg_search()} }
                        input {
                            r#type: "text",
                            class: "search-input",
                            placeholder: "Filter  (Cmd K for palette)",
                            value: "{search_val}",
                            oninput: move |e| state.write().sidebar_search = e.value().clone(),
                        }
                    }
                }
            }

            // Content
            div { class: "sidebar-content",
                if is_loading {
                    div { class: "skeleton-list",
                        div { class: "skeleton-item" }
                        div { class: "skeleton-item" }
                        div { class: "skeleton-item" }
                        div { class: "skeleton-item" }
                        div { class: "skeleton-item" }
                        div { class: "skeleton-item" }
                    }
                } else if !has_binary {
                    div { class: "sidebar-state",
                        div { class: "state-icon", {svg_binary()} }
                        span { class: "state-title", "No binary loaded" }
                        span { class: "state-sub", "Open a binary to begin" }
                    }
                } else if filtered.read().is_empty() {
                    div { class: "sidebar-state",
                        span { class: "state-title", "No results" }
                        span { class: "state-sub", "Try a different query" }
                    }
                } else {
                    // ── Virtual list ──────────────────────────────────────
                    ul {
                        class: "function-list",
                        // Approximate virtual scroll position via wheel delta.
                        // This avoids JS eval while still windowing correctly for most cases.
                        onwheel: move |e| {
                            let delta = e.delta().strip_units().y;
                            let mut st = scroll_top.write();
                            *st = (*st + delta).max(0.0).min(total_h - VISIBLE_H);
                        },

                        // Top spacer fills the invisible items above
                        li {
                            key: "spacer-top",
                            style: "height: {offset_top}px; padding: 0; pointer-events: none;",
                        }

                        // Visible window
                        for entry in filtered.read()[start..end].iter() {
                            {
                                let is_sel = selected == Some(entry.addr);
                                let display = if entry.name.is_empty() {
                                    format!("sub_{:x}", entry.addr)
                                } else {
                                    entry.name.clone()
                                };
                                let dot_cls = if entry.is_exp      { "fn-type-dot is-export" }
                                              else if entry.is_thunk
                                                   || entry.is_imp { "fn-type-dot is-import" }
                                              else                  { "fn-type-dot is-code" };
                                let item_cls = if is_sel { "function-item is-selected" }
                                               else      { "function-item" };
                                let e2 = entry.clone();
                                rsx! {
                                    li {
                                        key: "{entry.addr}",
                                        class: "{item_cls}",
                                        onclick: move |_| on_select(e2.clone()),
                                        div { class: "{dot_cls}" }
                                        div { class: "fn-info",
                                            div { class: "fn-name", "{display}" }
                                            div { class: "fn-addr", "0x{entry.addr:016x}" }
                                        }
                                        if entry.is_thunk {
                                            span { class: "fn-kind-pill kind-thunk", "THUNK" }
                                        } else if entry.is_exp {
                                            span { class: "fn-kind-pill kind-exp", "EXP" }
                                        } else if entry.is_imp {
                                            span { class: "fn-kind-pill kind-imp", "IMP" }
                                        }
                                    }
                                }
                            }
                        }

                        // Bottom spacer
                        if spacer_bot > 0.0 {
                            li {
                                key: "{spacer_bot}",
                                style: "height: {spacer_bot}px; padding: 0; pointer-events: none;",
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Shared async decompile helper ────────────────────────────────────────────

pub async fn run_decompile(
    mut state: Signal<AppState>,
    binary: Option<Arc<fission_loader::loader::LoadedBinary>>,
    addr: u64,
    name: String,
) {
    let result: Result<DecompileOutput, String> =
        crate::engine::run_decompile(binary, addr, name).await;

    match result {
        Ok(out) => {
            let bytes = out.code.len();
            let fell = out.fell_back;
            let reason = out.fallback_reason.clone();
            let has_cfg = out.cfg.is_some();
            let mut s = state.write();
            s.decompiled_code = Some(out.code);
            s.decompiled_nir = out.code_nir;
            s.current_cfg = out.cfg;
            s.is_decompiling = false;
            if fell {
                s.push_log(LogEntry::warn(format!(
                    "Fell back \u{2014} {}",
                    reason.as_deref().unwrap_or("unknown")
                )));
            } else {
                s.push_log(LogEntry::info(format!("Complete  ({bytes} bytes)")));
            }
            if has_cfg {
                s.push_log(LogEntry::info(
                    "CFG captured \u{2014} view in the CFG tab.".to_string(),
                ));
            }
        }
        Err(e) => {
            let mut s = state.write();
            s.is_decompiling = false;
            s.current_cfg = None;
            s.push_log(LogEntry::error(format!("Decompile failed: {e}")));
        }
    }
}
