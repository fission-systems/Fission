//! Xrefs viewer — shows callers and callees of the selected function.
//!
//! Xref data is loaded on-demand the first time the Xrefs tab is shown
//! after a function is selected, using `spawn_blocking`.

use crate::engine::{XrefRow, xrefs_for_function_blocking};
use crate::state::{LogEntry, use_app_state};
use dioxus::prelude::*;

#[component]
pub fn XrefsView() -> Element {
    let mut state = use_app_state();

    let fn_addr = state.read().current_function_addr;
    let is_loading = state.read().is_loading_xrefs;
    let callers = state.read().current_xref_callers.clone();
    let callees = state.read().current_xref_callees.clone();

    // Load on mount (or when fn_addr changes)
    let load_addr = fn_addr;
    use_effect(move || {
        let Some(addr) = load_addr else { return; };
        let binary = state.read().binary.clone();
        let Some(binary) = binary else { return; };

        // Only load if not already loading and no data yet
        {
            let s = state.read();
            if s.is_loading_xrefs { return; }
        }
        {
            let mut s = state.write();
            s.is_loading_xrefs = true;
            s.current_xref_callers.clear();
            s.current_xref_callees.clear();
        }

        spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                xrefs_for_function_blocking(&binary, addr)
            }).await;

            match result {
                Ok((callers, callees)) => {
                    let nc = callers.len();
                    let nk = callees.len();
                    let mut s = state.write();
                    s.current_xref_callers = callers;
                    s.current_xref_callees = callees;
                    s.is_loading_xrefs = false;
                    s.push_log(LogEntry::info(format!(
                        "Xrefs: {nc} callers, {nk} callees"
                    )));
                }
                Err(e) => {
                    let mut s = state.write();
                    s.is_loading_xrefs = false;
                    s.push_log(LogEntry::error(format!("Xref error: {e}")));
                }
            }
        });
    });

    let Some(_addr) = fn_addr else {
        return rsx! {
            div { class: "cfg-empty",
                span { class: "cfg-empty-title", "Xrefs" }
                span { class: "cfg-empty-sub", "Select a function to view cross-references." }
            }
        };
    };

    if is_loading {
        return rsx! {
            div { class: "cfg-empty",
                span { class: "cfg-empty-title", "Loading Xrefs\u{2026}" }
            }
        };
    }

    rsx! {
        div { class: "xrefs-panel",
            // ── Callers ────────────────────────────────────────────────────
            div { class: "xrefs-section",
                div { class: "xrefs-header",
                    span { class: "xrefs-header-label", "CALLERS" }
                    span { class: "xrefs-header-count", "{callers.len()}" }
                }
                if callers.is_empty() {
                    div { class: "xrefs-empty", "No known callers." }
                } else {
                    for (i, row) in callers.iter().enumerate() {
                        {
                            let label = row.fn_name.as_deref().unwrap_or("unknown");
                            let addr_str = format!("0x{:x}", row.from_addr);
                            let kind = row.kind.as_str();
                            rsx! {
                                div { class: "xrefs-row", key: "{i}",
                                    span { class: "xrefs-kind", "{kind}" }
                                    span { class: "xrefs-addr", "{addr_str}" }
                                    span { class: "xrefs-name", "{label}" }
                                }
                            }
                        }
                    }
                }
            }
            // ── Callees ────────────────────────────────────────────────────
            div { class: "xrefs-section",
                div { class: "xrefs-header",
                    span { class: "xrefs-header-label", "CALLEES" }
                    span { class: "xrefs-header-count", "{callees.len()}" }
                }
                if callees.is_empty() {
                    div { class: "xrefs-empty", "No known call targets." }
                } else {
                    for (i, row) in callees.iter().enumerate() {
                        {
                            let label = row.fn_name.as_deref()
                                .or(row.symbol.as_deref())
                                .unwrap_or("unknown");
                            let addr_str = row.to_addr
                                .map(|a| format!("0x{:x}", a))
                                .unwrap_or_else(|| "indirect".to_string());
                            let kind = row.kind.as_str();
                            rsx! {
                                div { class: "xrefs-row", key: "{i}",
                                    span { class: "xrefs-kind", "{kind}" }
                                    span { class: "xrefs-addr", "{addr_str}" }
                                    span { class: "xrefs-name", "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
