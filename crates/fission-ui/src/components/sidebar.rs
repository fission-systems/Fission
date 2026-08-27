//! Sidebar — function list with:
//!   • `use_memo` for O(1) re-render on unrelated state changes
//!   • Off-screen rows skipped by native CSS `content-visibility: auto`
//!     (see .function-item/.str-item) for paint/layout cost -- but that
//!     alone doesn't help VNode construction/diffing cost, which is the
//!     real lag at tens of thousands of functions (a real ~15k-function
//!     binary was the case that surfaced this). So the list is *also*
//!     incrementally mounted: only `sidebar_render_limit` rows are ever
//!     built, growing in batches as the user scrolls near the bottom.
//!     This is append-only (rows are only ever added past the end, never
//!     removed or repositioned), which is why it avoids the two failure
//!     modes an earlier windowed/virtual-scroll attempt hit here
//!     (wheel-delta tracking racing real scroll, and an onscroll listener
//!     fighting the browser's own scroll-position adjustments as rows were
//!     added/removed *above* the viewport) -- this design never touches
//!     scroll position or removes already-rendered content, so there's
//!     nothing for the browser's own scroll adjustment to fight.
//!   • Import / Thunk classification before decompile

use crate::state::{
    use_app_state, AppState, CachedDecompile, FunctionKind, LogEntry, SidebarKindFilter,
    SIDEBAR_RENDER_BATCH,
};
use dioxus::prelude::*;
use std::sync::Arc;

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

// ── Strings tab empty-state message ──────────────────────────────────────────
// On wasm32 (fission-web), string extraction runs server-side and isn't
// serialised over the API yet (`run_load`'s wasm arm hardcodes
// `strings: Vec::new()`), so an empty list there doesn't mean the binary
// truly has no strings -- it's a platform gap, and the message should say so
// rather than assert a fact about the binary that isn't actually known.
#[cfg(target_arch = "wasm32")]
fn no_strings_reason() -> &'static str {
    "Not available yet in the browser build -- string extraction runs server-side and isn't exposed over the API"
}
#[cfg(not(target_arch = "wasm32"))]
fn no_strings_reason() -> &'static str {
    "Binary has no extracted strings"
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
        let kf = s.sidebar_kind_filter.clone();
        s.functions
            .iter()
            .filter(|f| {
                // Kind filter
                let kind_ok = match &kf {
                    SidebarKindFilter::All => true,
                    SidebarKindFilter::Code => !f.is_import && !f.is_thunk_like && !f.is_export,
                    SidebarKindFilter::Export => f.is_export,
                    SidebarKindFilter::Import => f.is_import || f.is_thunk_like,
                };
                // Search filter
                let q_ok = q.is_empty()
                    || f.name.to_lowercase().contains(&q)
                    || format!("{:x}", f.address).contains(&q);
                kind_ok && q_ok
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

    // Inline rename state: Some(addr) when editing
    let rename_addr: Signal<Option<u64>> = use_signal(|| None);
    let rename_draft: Signal<String> = use_signal(|| String::new());

    // Grow the render window as the user scrolls near the bottom of the
    // (real, natively-scrolling) function list. Delegated on `document`
    // with `capture: true` rather than attached directly to the `<ul
    // id="function-list-scroll">` -- scroll events don't bubble, but the
    // capture phase still routes through `document` first regardless of
    // whether that element exists yet at listener-registration time (it
    // doesn't, the first time this runs: no binary is loaded when Sidebar
    // first mounts), so this works the same way click delegation would.
    use_hook(|| {
        spawn(async move {
            let mut eval = document::eval(
                r#"
                let last = 0;
                document.addEventListener('scroll', (e) => {
                    const el = e.target;
                    if (!el || el.id !== 'function-list-scroll') return;
                    if (el.scrollTop + el.clientHeight < el.scrollHeight - 400) return;
                    const now = Date.now();
                    if (now - last < 200) return;
                    last = now;
                    dioxus.send(1);
                }, true);
                "#,
            );
            loop {
                if eval.recv::<i32>().await.is_err() {
                    break;
                }
                let mut s = state.write();
                if s.sidebar_render_limit < filtered.read().len() {
                    s.sidebar_render_limit += SIDEBAR_RENDER_BATCH;
                }
            }
        });
    });

    // When sidebar_scroll_target changes (e.g. jumping here from an Xrefs
    // click-through), scroll the real DOM element into view. If the target
    // isn't within the currently-mounted batch, extend sidebar_render_limit
    // to cover it first and let this effect re-run (it's subscribed to the
    // whole `state` signal, so the render_limit write below re-triggers it)
    // once the DOM actually has the row.
    use_effect(move || {
        let target = state.read().sidebar_scroll_target;
        let Some(addr) = target else {
            return;
        };

        let idx = filtered.read().iter().position(|f| f.addr == addr);
        if let Some(idx) = idx {
            let limit = state.read().sidebar_render_limit;
            if idx >= limit {
                state.write().sidebar_render_limit = idx + SIDEBAR_RENDER_BATCH;
                return;
            }
        }

        document::eval(&format!(
            "document.getElementById('fn-item-{addr:x}')?.scrollIntoView({{block: 'center'}});"
        ));
        state.write().sidebar_scroll_target = None;
    });

    // Fetch the whole-binary call graph once, the first time the tab is
    // opened (not per-function like xrefs -- this is one fetch for the
    // whole table, cached in `call_graph_nodes` until the next binary
    // load). Unconditional at the top level like the effect above --
    // Dioxus requires every hook called on every render, so gating the
    // *call* to `use_effect` on `sidebar_tab == CallGraph` (rather than
    // gating what the effect body does) would reintroduce the exact hook-
    // ordering bug already fixed in editor.rs/cfg_view.rs this session.
    use_effect(move || {
        if state.read().sidebar_tab != crate::state::SidebarTab::CallGraph {
            return;
        }
        if state.read().call_graph_loaded || state.read().is_loading_callgraph {
            return;
        }
        let binary = state.read().binary.clone();
        let session = state.read().server_session_id.clone();
        state.write().is_loading_callgraph = true;
        spawn(async move {
            let result = crate::engine::run_callgraph(binary, session).await;
            let mut s = state.write();
            match result {
                Ok(nodes) => {
                    s.call_graph_nodes = nodes;
                    s.call_graph_loaded = true;
                }
                Err(e) => s.push_log(LogEntry::error(format!("Call graph failed: {e}"))),
            }
            s.is_loading_callgraph = false;
        });
    });

    let fn_total = filtered.read().len();
    let render_limit = state.read().sidebar_render_limit.min(fn_total);

    // ── Read once for rendering ───────────────────────────────────────────────
    let has_binary = state.read().has_binary_loaded();
    let is_loading = state.read().is_loading_binary;
    let is_analyzing = state.read().is_analyzing;
    let fn_count = fn_total;
    let search_val = state.read().sidebar_search.clone();
    let selected = state.read().current_function_addr;
    let active_tab = state.read().sidebar_tab.clone();
    let str_search = state.read().strings_search.clone();
    let rename_now = *rename_addr.read();

    // Strings list (filtered)
    let strings_list: Vec<crate::state::BinaryString> = {
        let s = state.read();
        let q = str_search.to_lowercase();
        s.strings
            .iter()
            .filter(|bs| {
                q.is_empty()
                    || bs.value.to_lowercase().contains(&q)
                    || bs.section.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    };
    let str_total = strings_list.len();

    // Call graph list (filtered + sorted). Capped to CALLGRAPH_RENDER_LIMIT
    // rows for the same reason the function list is incrementally mounted
    // (see this file's module doc) -- a real binary can have tens of
    // thousands of functions, and since the list is already sorted by
    // caller/callee count, the top slice is the interesting part anyway;
    // a search narrows past the cap same as the Functions tab's filter.
    const CALLGRAPH_RENDER_LIMIT: usize = 500;
    let cg_search = state.read().call_graph_search.clone();
    let cg_sort = state.read().call_graph_sort.clone();
    let is_loading_callgraph = state.read().is_loading_callgraph;
    let (call_graph_list, cg_matched_total): (Vec<crate::engine::CallGraphNode>, usize) = {
        let s = state.read();
        let q = cg_search.to_lowercase();
        let mut list: Vec<crate::engine::CallGraphNode> = s
            .call_graph_nodes
            .iter()
            .filter(|n| {
                q.is_empty()
                    || n.name.to_lowercase().contains(&q)
                    || format!("{:x}", n.address).contains(&q)
            })
            .cloned()
            .collect();
        match cg_sort {
            crate::state::CallGraphSort::CallersDesc => list.sort_by(|a, b| {
                b.caller_count
                    .cmp(&a.caller_count)
                    .then_with(|| a.name.cmp(&b.name))
            }),
            crate::state::CallGraphSort::CalleesDesc => list.sort_by(|a, b| {
                b.callee_count
                    .cmp(&a.callee_count)
                    .then_with(|| a.name.cmp(&b.name))
            }),
            crate::state::CallGraphSort::NameAsc => list.sort_by(|a, b| a.name.cmp(&b.name)),
        }
        let matched_total = list.len();
        list.truncate(CALLGRAPH_RENDER_LIMIT);
        (list, matched_total)
    };

    rsx! {
        div { class: "sidebar",
            // ── Tab switcher ────────────────────────────────────────────────
            div { class: "sidebar-tabs",
                div {
                    class: if active_tab == crate::state::SidebarTab::Functions { "sidebar-tab is-active" } else { "sidebar-tab" },
                    onclick: move |_| state.write().sidebar_tab = crate::state::SidebarTab::Functions,
                    "Functions"
                    if has_binary {
                        span { class: "sidebar-tab-count", "{fn_count}" }
                    }
                    if is_analyzing {
                        span { class: "decompiling-label sidebar-analyzing-label", "analyzing…" }
                    }
                }
                div {
                    class: if active_tab == crate::state::SidebarTab::Strings { "sidebar-tab is-active" } else { "sidebar-tab" },
                    onclick: move |_| state.write().sidebar_tab = crate::state::SidebarTab::Strings,
                    "Strings"
                    if has_binary {
                        span { class: "sidebar-tab-count", "{str_total}" }
                    }
                }
                div {
                    class: if active_tab == crate::state::SidebarTab::CallGraph { "sidebar-tab is-active" } else { "sidebar-tab" },
                    onclick: move |_| state.write().sidebar_tab = crate::state::SidebarTab::CallGraph,
                    "Call Graph"
                    if has_binary && state.read().call_graph_loaded {
                        span { class: "sidebar-tab-count", "{state.read().call_graph_nodes.len()}" }
                    }
                }
            }

            // ── Search bar (shared, changes query target by tab) ─────────────
            if has_binary && active_tab == crate::state::SidebarTab::Functions {
                {
                    let kf = state.read().sidebar_kind_filter.clone();
                    let chip = |label: &'static str, filter: SidebarKindFilter| -> Element {
                        let active = kf == filter;
                        let cls = if active { "kind-chip is-active" } else { "kind-chip" };
                        rsx! {
                            button {
                                class: "{cls}",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.sidebar_kind_filter = filter.clone();
                                    s.sidebar_render_limit = SIDEBAR_RENDER_BATCH;
                                },
                                "{label}"
                            }
                        }
                    };
                    rsx! {
                        div { class: "kind-filter-bar",
                            {chip("All", SidebarKindFilter::All)}
                            {chip("Code", SidebarKindFilter::Code)}
                            {chip("Exp", SidebarKindFilter::Export)}
                            {chip("Imp", SidebarKindFilter::Import)}
                        }
                    }
                }
            }

            if has_binary && active_tab == crate::state::SidebarTab::CallGraph {
                {
                    let sort_chip = |label: &'static str, sort: crate::state::CallGraphSort| -> Element {
                        let active = cg_sort == sort;
                        let cls = if active { "kind-chip is-active" } else { "kind-chip" };
                        rsx! {
                            button {
                                class: "{cls}",
                                onclick: move |_| state.write().call_graph_sort = sort.clone(),
                                "{label}"
                            }
                        }
                    };
                    rsx! {
                        div { class: "kind-filter-bar",
                            {sort_chip("Most called", crate::state::CallGraphSort::CallersDesc)}
                            {sort_chip("Most calling", crate::state::CallGraphSort::CalleesDesc)}
                            {sort_chip("Name", crate::state::CallGraphSort::NameAsc)}
                        }
                    }
                }
            }

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
                                oninput: move |e| {
                                    let mut s = state.write();
                                    s.sidebar_search = e.value().clone();
                                    s.sidebar_render_limit = SIDEBAR_RENDER_BATCH;
                                },
                            }
                        } else if active_tab == crate::state::SidebarTab::CallGraph {
                            input {
                                r#type: "text",
                                class: "search-input",
                                placeholder: "Search call graph…",
                                value: "{cg_search}",
                                oninput: move |e| state.write().call_graph_search = e.value().clone(),
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
                        span { class: "state-sub", "Drag & drop a binary here, or use File \u{2192} Open Binary\u{2026}" }
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
                            id: "function-list-scroll",
                            class: "function-list",

                            for entry in filtered.read().iter().take(render_limit) {
                                {
                                    let addr = entry.addr;
                                    let is_sel = selected == Some(addr);
                                    let is_rename = rename_now == Some(addr);
                                    // Display name = rename_map override or original.
                                    // Still an O(n) per-render lookup, but plain data
                                    // (a HashMap probe + maybe a String clone) --
                                    // cheap compared to the VNode tree construction
                                    // FunctionListItem's own #[component] boundary
                                    // now lets Dioxus skip entirely for the ~8000
                                    // other items whose props didn't change.
                                    let display = {
                                        let s = state.read();
                                        s.rename_map.get(&addr).cloned()
                                            .unwrap_or_else(|| {
                                                if entry.name.is_empty() {
                                                    format!("sub_{addr:x}")
                                                } else {
                                                    entry.name.clone()
                                                }
                                            })
                                    };
                                    let orig_name = if entry.name.is_empty() {
                                        format!("sub_{addr:x}")
                                    } else {
                                        entry.name.clone()
                                    };

                                    rsx! {
                                        FunctionListItem {
                                            key: "{addr}",
                                            entry: entry.clone(),
                                            is_selected: is_sel,
                                            is_renaming: is_rename,
                                            display_name: display,
                                            orig_name,
                                            state,
                                            rename_addr,
                                            rename_draft,
                                        }
                                    }
                                }
                            }
                            if render_limit < fn_total {
                                li { class: "fn-list-more-hint",
                                    "Showing {render_limit} of {fn_total} \u{2014} scroll for more"
                                }
                            }
                        }
                    }
                } else if active_tab == crate::state::SidebarTab::CallGraph {
                    // ── Call Graph tab ──────────────────────────────────────────
                    if is_loading_callgraph {
                        div { class: "skeleton-list",
                            div { class: "skeleton-item" }
                            div { class: "skeleton-item" }
                            div { class: "skeleton-item" }
                        }
                    } else if call_graph_list.is_empty() {
                        div { class: "sidebar-state",
                            span { class: "state-title", "No results" }
                            span { class: "state-sub",
                                if cg_search.is_empty() {
                                    "No call-graph data for this binary."
                                } else {
                                    "No match for your query"
                                }
                            }
                        }
                    } else {
                        ul {
                            class: "function-list",
                            for node in call_graph_list.iter() {
                                {
                                    let addr = node.address;
                                    let is_sel = selected == Some(addr);
                                    let display = if node.name.is_empty() {
                                        format!("sub_{addr:x}")
                                    } else {
                                        node.name.clone()
                                    };
                                    let item_cls = if is_sel { "function-item is-selected" } else { "function-item" };
                                    rsx! {
                                        li {
                                            key: "{addr}",
                                            class: "{item_cls}",
                                            onclick: move |_| {
                                                spawn(async move {
                                                    navigate_to_address(state, addr).await;
                                                });
                                            },
                                            div { class: "fn-type-dot is-code" }
                                            div { class: "fn-info",
                                                div { class: "fn-name", "{display}" }
                                                div { class: "fn-addr", "0x{addr:016x}" }
                                            }
                                            span { class: "cg-counts", title: "callers \u{2190} / callees \u{2192}",
                                                "{node.caller_count}\u{2190} {node.callee_count}\u{2192}"
                                            }
                                        }
                                    }
                                }
                            }
                            if cg_matched_total > CALLGRAPH_RENDER_LIMIT {
                                li { class: "fn-list-more-hint",
                                    "Showing top {CALLGRAPH_RENDER_LIMIT} of {cg_matched_total} \u{2014} refine search to narrow"
                                }
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
                                    "{no_strings_reason()}"
                                } else {
                                    "No match for your query"
                                }
                            }
                        }
                    } else {
                        ul {
                            class: "str-list",

                            for bs in strings_list.iter() {
                                {
                                    let va      = bs.offset;
                                    let val     = bs.value.clone();
                                    let section = bs.section.clone();
                                    // Truncate by *character* count, not byte
                                    // index: `binary.string_map`-sourced values
                                    // can hold decoded non-ASCII text (e.g. a
                                    // PE's UTF-16 strings converted to UTF-8),
                                    // and `&val[..64]` would panic outright if
                                    // byte 64 fell inside a multi-byte
                                    // character instead of on a boundary.
                                    let val_disp = if val.chars().count() > 64 {
                                        let truncated: String = val.chars().take(64).collect();
                                        format!("{truncated}…")
                                    } else {
                                        val.clone()
                                    };
                                    rsx! {
                                        li {
                                            key: "{va}",
                                            class: "str-item",
                                            onclick: move |_| {
                                                // Switch to Hex tab and scroll to this string's offset.
                                                let mut s = state.write();
                                                s.hex_view_target = Some(va);
                                                s.active_tab = crate::state::EditorTab::Hex;
                                            },
                                            div { class: "fn-type-dot is-code" }
                                            div { class: "fn-info",
                                                div { class: "str-value", title: "{val}", "{val_disp}" }
                                                div { class: "str-meta",
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
                        }
                    }
                }
            }
        }
    }
}

/// One function-list row, its own `#[component]` boundary specifically so
/// Dioxus can memoize it: with ~8000 functions in a real binary, building a
/// fresh multi-child VNode tree (with event handlers) for every row on every
/// `Sidebar` re-render -- even one triggered by an unrelated state change,
/// like renaming a *different* function -- was the actual rendering cost;
/// `content-visibility: auto` (see module doc) only ever addressed paint/
/// layout cost for off-screen rows, not VNode construction/diffing cost for
/// the whole list. As its own component, Dioxus compares this row's props
/// (`entry`/`is_selected`/`is_renaming`/`display_name`, all plain data) to
/// last render and skips rebuilding its subtree entirely when they're
/// unchanged -- which is the case for effectively every row on most
/// re-renders (only the previously- and newly-selected/renamed rows
/// actually change).
#[component]
fn FunctionListItem(
    entry: FnEntry,
    is_selected: bool,
    is_renaming: bool,
    display_name: String,
    orig_name: String,
    state: Signal<AppState>,
    mut rename_addr: Signal<Option<u64>>,
    mut rename_draft: Signal<String>,
) -> Element {
    let addr = entry.addr;
    let dot_cls = if entry.is_exp {
        "fn-type-dot is-export"
    } else if entry.is_thunk || entry.is_imp {
        "fn-type-dot is-import"
    } else {
        "fn-type-dot is-code"
    };
    let item_cls = if is_selected {
        "function-item is-selected"
    } else {
        "function-item"
    };
    let entry_for_click = entry.clone();

    rsx! {
        li {
            id: "fn-item-{addr:x}",
            class: "{item_cls}",
            onclick: move |_| {
                if !is_renaming {
                    select_function(state, entry_for_click.clone());
                }
            },
            ondoubleclick: move |e| {
                e.stop_propagation();
                rename_draft.set(display_name.clone());
                rename_addr.set(Some(addr));
            },
            div { class: "{dot_cls}" }
            div { class: "fn-info",
                if is_renaming {
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
                    div { class: "fn-name", "{display_name}" }
                    div { class: "fn-addr", "0x{addr:016x}" }
                }
            }
            if !is_renaming {
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

/// Resolve a clicked function's kind and either show an import stub or
/// kick off an async decompile. Free function (not a parent-scoped
/// closure) so `FunctionListItem`'s `onclick` doesn't need a callback
/// prop -- see that component's doc comment for why avoiding extra props
/// here matters at ~8000 rows.
fn select_function(mut state: Signal<AppState>, entry: FnEntry) {
    // Import thunks (is_imp=true, is_thunk=true) are IAT stubs — JMP [IAT].
    // They produce self-recursive pseudocode if decompiled, so treat them
    // exactly like plain imports: show the stub comment, skip decompile.
    let kind = if entry.is_imp {
        // Covers both pure imports (is_thunk=false) and import thunks (is_thunk=true)
        FunctionKind::Import {
            library: entry.lib.clone(),
        }
    } else if entry.is_thunk {
        // Non-import thunks (e.g. vtable thunks, tail-call thunks) — still try to decompile
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
        s.navigate_to(entry.addr);
    }

    match kind {
        FunctionKind::Import { library } => {
            let lib_str = library.as_deref().unwrap_or("unknown");
            let thunk_t = entry.thunk_t;
            let is_thunk = entry.is_thunk;
            let kind_tag = if is_thunk {
                "Import thunk (IAT stub)"
            } else {
                "Import stub"
            };
            let thunk_line = thunk_t
                .map(|t| format!("\n *  IAT target: 0x{t:x}"))
                .unwrap_or_default();
            let stub = format!(
                "/* {kind_tag} — no decompilable body.\n\
                 *\n\
                 *  Symbol  : {}\n\
                 *  Address : 0x{:016x}\n\
                 *  Library : {lib_str}{thunk_line}\n\
                 *\n\
                 *  This is a JMP [IAT] thunk. The real implementation\n\
                 *  lives in the imported DLL, not in this binary.\n\
                 */",
                entry.name, entry.addr,
            );
            let mut s = state.write();
            s.decompiled_code = Some(stub);
            if is_thunk {
                let tgt_str = thunk_t.map(|t| format!(" → 0x{t:x}")).unwrap_or_default();
                s.push_log(LogEntry::info(format!(
                    "Import thunk: {}{tgt_str}  (from {lib_str})",
                    entry.name
                )));
            } else {
                s.push_log(LogEntry::info(format!(
                    "Import stub: {}  (from {lib_str})",
                    entry.name
                )));
            }
        }
        FunctionKind::Thunk { target } => {
            let name = entry.name.clone();
            let addr = entry.addr;
            if apply_cached_decompile(state, addr) {
                let tgt = target.map(|t| format!("0x{t:x}")).unwrap_or_default();
                state.write().push_log(LogEntry::warn(format!(
                    "\"{name}\" is an import thunk — output will appear self-recursive \
                     (IAT target: {tgt})"
                )));
                return;
            }
            let tgt = target.map(|t| format!("0x{t:x}")).unwrap_or_default();
            {
                let mut s = state.write();
                s.is_decompiling = true;
                s.push_log(LogEntry::warn(format!(
                    "\"{name}\" is an import thunk — output will appear self-recursive \
                     (IAT target: {tgt})"
                )));
            }
            let binary = state.read().binary.clone();
            let session = state.read().server_session_id.clone();
            spawn(async move { run_decompile(state, binary, session, addr, name).await });
        }
        FunctionKind::Code => {
            let name = entry.name.clone();
            let addr = entry.addr;
            if apply_cached_decompile(state, addr) {
                state
                    .write()
                    .push_log(LogEntry::info(format!("{name}  @  0x{addr:x}  (cached)")));
                return;
            }
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
}

// ── Shared navigation helper ─────────────────────────────────────────────────

/// Navigate to a function by address from anywhere in the UI (e.g. clicking a
/// `sub_XXXX` reference inside decompiled pseudocode, a command-palette
/// selection, or a Back/Forward step) -- resolves the target's kind from the
/// current function list and either shows an import stub or kicks off a real
/// decompile. `record_history` controls whether this call appends a new
/// `nav_history` entry (a real navigation, e.g. clicking a reference) or not
/// (Back/Forward stepping through history that's already been updated by
/// `nav_back`/`nav_forward`, which also already set `sidebar_scroll_target`).
async fn navigate_to_address_impl(
    mut state: Signal<AppState>,
    addr: u64,
    hint_name: Option<String>,
    record_history: bool,
) {
    let binary = state.read().binary.clone();
    let session = state.read().server_session_id.clone();

    let (kind, resolved_name) = {
        let s = state.read();
        if let Some(fi) = s.functions.iter().find(|f| f.address == addr) {
            let k = if fi.is_import && !fi.is_thunk_like {
                FunctionKind::Import {
                    library: fi.external_library.clone(),
                }
            } else if fi.is_thunk_like {
                FunctionKind::Thunk {
                    target: fi.thunk_target,
                }
            } else {
                FunctionKind::Code
            };
            (k, fi.name.clone())
        } else {
            // Not a known function (e.g. an xref target only known by
            // symbol name) -- prefer the caller's hint over a generic label.
            (
                FunctionKind::Code,
                hint_name.unwrap_or_else(|| format!("sub_{addr:x}")),
            )
        }
    };

    {
        let mut s = state.write();
        s.current_function_addr = Some(addr);
        s.current_function_kind = kind.clone();
        s.decompiled_code = None;
        s.decompiled_nir = None;
        s.current_cfg = None;
        s.current_xref_callers.clear();
        s.current_xref_callees.clear();
        s.is_loading_xrefs = false;
        if record_history {
            s.navigate_to(addr);
        }
        s.push_log(LogEntry::info(format!(
            "Jumped to {resolved_name}  @  0x{addr:x}"
        )));
    }

    match kind {
        FunctionKind::Import { library } => {
            let lib_str = library.as_deref().unwrap_or("unknown");
            let stub = format!(
                "/* Import stub \u{2014} no decompilable body.\n\
                 *\n\
                 *  Symbol  : {resolved_name}\n\
                 *  Address : 0x{addr:016x}\n\
                 *  Library : {lib_str}\n\
                 */"
            );
            state.write().decompiled_code = Some(stub);
        }
        FunctionKind::Thunk { .. } | FunctionKind::Code => {
            if apply_cached_decompile(state, addr) {
                return;
            }
            state.write().is_decompiling = true;
            run_decompile(state, binary, session, addr, resolved_name).await;
        }
    }
}

/// Navigate to `addr`, recording a new nav-history entry. Use for real
/// navigations: clicking a `sub_XXXX` reference or a command palette result.
pub async fn navigate_to_address(state: Signal<AppState>, addr: u64) {
    navigate_to_address_impl(state, addr, None, true).await;
}

/// Like [`navigate_to_address`], but with a fallback display name to use if
/// `addr` isn't in the current function list (e.g. an xrefs row that only
/// resolved to a symbol name, not a local function).
pub async fn navigate_to_address_with_hint(
    state: Signal<AppState>,
    addr: u64,
    hint_name: Option<String>,
) {
    navigate_to_address_impl(state, addr, hint_name, true).await;
}

/// Navigate to `addr` *without* touching nav history. Use for Back/Forward
/// stepping, where `AppState::nav_back`/`nav_forward` already updated
/// `nav_history`/`nav_cursor` themselves.
pub async fn navigate_to_address_no_history(state: Signal<AppState>, addr: u64) {
    navigate_to_address_impl(state, addr, None, false).await;
}

/// If `addr` has a cached decompile result, restore it into `AppState`
/// directly and return `true` (skips `is_decompiling`/network entirely --
/// re-selecting a function already visited this session is instant instead
/// of re-running the whole decompile). Returns `false` on a cache miss, in
/// which case the caller proceeds with a normal `run_decompile` call.
fn apply_cached_decompile(mut state: Signal<AppState>, addr: u64) -> bool {
    let mut s = state.write();
    let Some(cached) = s.decompile_cache.get(&addr).cloned() else {
        return false;
    };
    s.decompiled_code = Some(cached.code);
    s.decompiled_nir = cached.code_nir;
    s.current_cfg = cached.cfg;
    s.is_decompiling = false;
    true
}

// ── Shared async decompile helper ────────────────────────────────────────────

/// Native path: reads cached_facts from AppState, uses decompile_blocking_with_facts,
/// and stores the (re-)used FactStore back into AppState for future calls.
/// First call: builds FactStore once (seconds). Subsequent calls: reuse (milliseconds).
#[cfg(not(target_arch = "wasm32"))]
pub async fn run_decompile(
    mut state: Signal<AppState>,
    binary: Option<Arc<fission_loader::loader::LoadedBinary>>,
    _session_id: Option<String>,
    addr: u64,
    name: String,
) {
    use crate::engine::{build_facts_blocking, decompile_blocking_with_facts, FactStore};

    // Grab the cached FactStore (cheap Arc clone) before acquiring write lock
    let cached_facts = state.read().cached_facts.clone();

    let Some(binary) = binary else {
        state.write().is_decompiling = false;
        state.write().push_log(LogEntry::error("No binary loaded"));
        return;
    };

    let name_clone = name.clone();
    let result: Result<(crate::engine::DecompileOutput, Arc<FactStore>), String> =
        tokio::task::spawn_blocking(move || {
            let facts: Arc<FactStore> = cached_facts.unwrap_or_else(|| {
                // First call: build FactStore from binary (expensive, once)
                Arc::new(build_facts_blocking(binary.as_ref()))
            });
            let out = decompile_blocking_with_facts(binary.as_ref(), &facts, addr, &name_clone)?;
            Ok((out, facts))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);

    match result {
        Ok((out, facts)) => {
            let bytes = out.code.len();
            let fell = out.fell_back;
            let reason = out.fallback_reason.clone();
            let has_cfg = out.cfg.is_some();
            let mut s = state.write();
            s.decompile_cache.insert(
                addr,
                CachedDecompile {
                    code: out.code.clone(),
                    code_nir: out.code_nir.clone(),
                    cfg: out.cfg.clone(),
                },
            );
            s.decompiled_code = Some(out.code);
            s.decompiled_nir = out.code_nir;
            s.current_cfg = out.cfg;
            s.is_decompiling = false;
            // Store the FactStore for subsequent calls (O(1) Arc clone)
            s.cached_facts = Some(facts);
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

/// WASM path: delegates to HTTP server, no local FactStore cache.
#[cfg(target_arch = "wasm32")]
pub async fn run_decompile(
    mut state: Signal<AppState>,
    binary: Option<Arc<fission_loader::loader::LoadedBinary>>,
    session_id: Option<String>,
    addr: u64,
    name: String,
) {
    let result: Result<crate::engine::DecompileOutput, String> =
        crate::engine::run_decompile(binary, session_id, addr, name).await;

    match result {
        Ok(out) => {
            let bytes = out.code.len();
            let fell = out.fell_back;
            let reason = out.fallback_reason.clone();
            let has_cfg = out.cfg.is_some();
            let mut s = state.write();
            s.decompile_cache.insert(
                addr,
                CachedDecompile {
                    code: out.code.clone(),
                    code_nir: out.code_nir.clone(),
                    cfg: out.cfg.clone(),
                },
            );
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
