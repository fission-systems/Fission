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
    let mut str_scroll_top: Signal<f64> = use_signal(|| 0.0);

    // Inline rename state: Some(addr) when editing
    let mut rename_addr: Signal<Option<u64>> = use_signal(|| None);
    let mut rename_draft: Signal<String>     = use_signal(|| String::new());

    // When sidebar_scroll_target changes, adjust virtual scroll position
    use_effect(move || {
        let target = state.read().sidebar_scroll_target;
        let Some(addr) = target else { return; };
        let idx = filtered.read().iter().position(|e| e.addr == addr);
        if let Some(idx) = idx {
            let desired_top = (idx as f64 * ITEM_H - VISIBLE_H / 2.0 + ITEM_H / 2.0).max(0.0);
            *scroll_top.write() = desired_top;
        }
        state.write().sidebar_scroll_target = None;
    });

    // ── Derived slice for function list ──────────────────────────────────────
    let fn_total = filtered.read().len();
    let fn_start = ((*scroll_top.read() / ITEM_H) as usize).saturating_sub(OVERSCAN);
    let fn_visible = (VISIBLE_H / ITEM_H).ceil() as usize + OVERSCAN * 2;
    let fn_end = (fn_start + fn_visible).min(fn_total);

    let fn_total_h   = fn_total as f64 * ITEM_H;
    let fn_offset_top = fn_start as f64 * ITEM_H;
    let fn_spacer_bot = if fn_end < fn_total { (fn_total - fn_end) as f64 * ITEM_H } else { 0.0 };

    // ── Decompile on click ───────────────────────────────────────────────────
    let mut on_select = move |entry: FnEntry| {
        // Close any open rename
        rename_addr.set(None);

        let kind = if entry.is_imp && !entry.is_thunk {
            FunctionKind::Import { library: entry.lib.clone() }
        } else if entry.is_thunk {
            FunctionKind::Thunk { target: entry.thunk_t }
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
            s.navigate_to(entry.addr);
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
                    "Import stub: {}  (from {lib_str})", entry.name
                )));
            }
            FunctionKind::Thunk { target } => {
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
                let binary  = state.read().binary.clone();
                let session = state.read().server_session_id.clone();
                spawn(async move { run_decompile(state, binary, session, addr, name).await });
            }
            FunctionKind::Code => {
                let name    = entry.name.clone();
                let addr    = entry.addr;
                let session = state.read().server_session_id.clone();
                {
                    let mut s = state.write();
                    s.is_decompiling = true;
                    s.push_log(LogEntry::info(format!("Decompiling {name}  @  0x{addr:x}")));
                }
                let binary = state.read().binary.clone();
                spawn(async move { run_decompile(state, binary, session, addr, name).await });
            }
        }
    };

    // ── Read once for rendering ───────────────────────────────────────────────
    let has_binary   = state.read().binary.is_some();
    let is_loading   = state.read().is_loading_binary;
    let fn_count     = fn_total;
    let search_val   = state.read().sidebar_search.clone();
    let selected     = state.read().current_function_addr;
    let active_tab   = state.read().sidebar_tab.clone();
    let str_search   = state.read().strings_search.clone();
    let rename_now   = *rename_addr.read();

    // Strings list (filtered)
    let strings_list: Vec<crate::state::BinaryString> = {
        let s = state.read();
        let q = str_search.to_lowercase();
        s.strings.iter()
            .filter(|bs| q.is_empty() || bs.value.to_lowercase().contains(&q) || bs.section.to_lowercase().contains(&q))
            .cloned()
            .collect()
    };
    let str_total    = strings_list.len();
    let str_item_h   = 36.0_f64;
    let str_visible_h = VISIBLE_H;
    let str_start    = ((*str_scroll_top.read() / str_item_h) as usize).saturating_sub(OVERSCAN);
    let str_vis      = (str_visible_h / str_item_h).ceil() as usize + OVERSCAN * 2;
    let str_end      = (str_start + str_vis).min(str_total);
    let str_total_h  = str_total as f64 * str_item_h;
    let str_off_top  = str_start as f64 * str_item_h;
    let str_spc_bot  = if str_end < str_total { (str_total - str_end) as f64 * str_item_h } else { 0.0 };

    rsx! {
        div { class: "sidebar",
            // ── Tab switcher ────────────────────────────────────────────────
            div { class: "sidebar-tab-bar",
                div {
                    class: if active_tab == crate::state::SidebarTab::Functions { "sidebar-tab is-active" } else { "sidebar-tab" },
                    onclick: move |_| state.write().sidebar_tab = crate::state::SidebarTab::Functions,
                    "Functions"
                    if has_binary {
                        span { class: "sidebar-tab-badge", "{fn_count}" }
                    }
                }
                div {
                    class: if active_tab == crate::state::SidebarTab::Strings { "sidebar-tab is-active" } else { "sidebar-tab" },
                    onclick: move |_| state.write().sidebar_tab = crate::state::SidebarTab::Strings,
                    "Strings"
                    if has_binary {
                        span { class: "sidebar-tab-badge", "{str_total}" }
                    }
                }
            }

            // ── Search bar (shared, changes query target by tab) ─────────────
            if has_binary {
                div { class: "sidebar-search",
                    div { class: "search-wrap",
                        span { class: "search-icon", {svg_search()} }
                        if active_tab == crate::state::SidebarTab::Functions {
                            input {
                                r#type: "text",
                                class: "search-input",
                                placeholder: "Filter  (Cmd K for palette)",
                                value: "{search_val}",
                                oninput: move |e| state.write().sidebar_search = e.value().clone(),
                            }
                        } else {
                            input {
                                r#type: "text",
                                class: "search-input",
                                placeholder: "Search strings…",
                                value: "{str_search}",
                                oninput: move |e| state.write().strings_search = e.value().clone(),
                            }
                        }
                    }
                }
            }

            // ── Content ─────────────────────────────────────────────────────
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
                } else if active_tab == crate::state::SidebarTab::Functions {
                    // ── Functions tab ──────────────────────────────────────────
                    if filtered.read().is_empty() {
                        div { class: "sidebar-state",
                            span { class: "state-title", "No results" }
                            span { class: "state-sub", "Try a different query" }
                        }
                    } else {
                        ul {
                            class: "function-list",
                            onwheel: move |e| {
                                let delta = e.delta().strip_units().y;
                                let mut st = scroll_top.write();
                                *st = (*st + delta).max(0.0).min(fn_total_h - VISIBLE_H);
                            },
                            li { key: "spacer-top", style: "height: {fn_offset_top}px; padding:0; pointer-events:none;" }

                            for entry in filtered.read()[fn_start..fn_end].iter() {
                                {
                                    let is_sel   = selected == Some(entry.addr);
                                    let addr     = entry.addr;
                                    let is_rename= rename_now == Some(addr);

                                    // Display name = rename_map override or original
                                    let display = {
                                        let s = state.read();
                                        s.rename_map.get(&addr).cloned()
                                            .unwrap_or_else(|| {
                                                if entry.name.is_empty() {
                                                    format!("sub_{:x}", addr)
                                                } else {
                                                    entry.name.clone()
                                                }
                                            })
                                    };
                                    let orig_name = if entry.name.is_empty() {
                                        format!("sub_{:x}", addr)
                                    } else {
                                        entry.name.clone()
                                    };
                                    let dot_cls = if entry.is_exp     { "fn-type-dot is-export" }
                                                  else if entry.is_thunk || entry.is_imp { "fn-type-dot is-import" }
                                                  else { "fn-type-dot is-code" };
                                    let item_cls = if is_sel { "function-item is-selected" } else { "function-item" };
                                    let e2 = entry.clone();

                                    rsx! {
                                        li {
                                            key: "{addr}",
                                            class: "{item_cls}",
                                            onclick: move |_| {
                                                if !is_rename {
                                                    on_select(e2.clone());
                                                }
                                            },
                                            ondoubleclick: move |e| {
                                                e.stop_propagation();
                                                rename_draft.set(display.clone());
                                                rename_addr.set(Some(addr));
                                            },
                                            div { class: "{dot_cls}" }
                                            div { class: "fn-info",
                                                if is_rename {
                                                    // Inline rename input
                                                    input {
                                                        class: "fn-rename-input",
                                                        r#type: "text",
                                                        value: "{rename_draft}",
                                                        autofocus: true,
                                                        oninput: move |ev| rename_draft.set(ev.value().clone()),
                                                        onkeydown: move |ev| {
                                                            match ev.key() {
                                                                Key::Enter => {
                                                                    let new_name = rename_draft.read().clone();
                                                                    if !new_name.is_empty() && new_name != orig_name {
                                                                        state.write().rename_map.insert(addr, new_name);
                                                                    } else if new_name.is_empty() {
                                                                        // empty = remove override
                                                                        state.write().rename_map.remove(&addr);
                                                                    }
                                                                    rename_addr.set(None);
                                                                }
                                                                Key::Escape => { rename_addr.set(None); }
                                                                _ => {}
                                                            }
                                                        },
                                                        onblur: move |_| { rename_addr.set(None); },
                                                    }
                                                } else {
                                                    div { class: "fn-name", "{display}" }
                                                    div { class: "fn-addr", "0x{addr:016x}" }
                                                }
                                            }
                                            if !is_rename {
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
                            }

                            if fn_spacer_bot > 0.0 {
                                li { key: "{fn_spacer_bot}", style: "height: {fn_spacer_bot}px; padding:0; pointer-events:none;" }
                            }
                        }
                    }
                } else {
                    // ── Strings tab ────────────────────────────────────────────
                    if strings_list.is_empty() {
                        div { class: "sidebar-state",
                            span { class: "state-title", "No strings" }
                            span { class: "state-sub",
                                if str_search.is_empty() {
                                    "Binary has no extracted strings"
                                } else {
                                    "No match for your query"
                                }
                            }
                        }
                    } else {
                        ul {
                            class: "function-list strings-list",
                            onwheel: move |e| {
                                let delta = e.delta().strip_units().y;
                                let mut st = str_scroll_top.write();
                                *st = (*st + delta).max(0.0).min(str_total_h - str_visible_h);
                            },
                            li { key: "str-spacer-top", style: "height: {str_off_top}px; padding:0; pointer-events:none;" }

                            for bs in strings_list[str_start..str_end].iter() {
                                {
                                    let va      = bs.offset;
                                    let val     = bs.value.clone();
                                    let section = bs.section.clone();
                                    let val_disp = if val.len() > 64 {
                                        format!("{}…", &val[..64])
                                    } else {
                                        val.clone()
                                    };
                                    rsx! {
                                        li {
                                            key: "{va}",
                                            class: "function-item string-item",
                                            onclick: move |_| {
                                                // Switch to Hex tab and (future) scroll to offset
                                                let mut s = state.write();
                                                s.active_tab = crate::state::EditorTab::Hex;
                                            },
                                            div { class: "fn-type-dot is-code" }
                                            div { class: "fn-info",
                                                div { class: "fn-name str-val", title: "{val}", "{val_disp}" }
                                                div { class: "fn-addr",
                                                    if !section.is_empty() {
                                                        span { class: "str-section", "{section}  " }
                                                    }
                                                    "0x{va:x}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if str_spc_bot > 0.0 {
                                li { key: "{str_spc_bot}", style: "height: {str_spc_bot}px; padding:0; pointer-events:none;" }
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
    session_id: Option<String>,
    addr: u64,
    name: String,
) {
    let result: Result<DecompileOutput, String> =
        crate::engine::run_decompile(binary, session_id, addr, name).await;

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
