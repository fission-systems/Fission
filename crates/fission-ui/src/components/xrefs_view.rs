//! Xrefs viewer — shows callers and callees of the selected function.
//!
//! Clicking a row navigates to the target function: updates selection,
//! triggers auto-decompile, and switches the bottom panel to Output.

use crate::components::sidebar::run_decompile;
use crate::engine::XrefRow;
use crate::state::{use_app_state, BottomTab, FunctionKind, LogEntry};
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
        let Some(addr) = load_addr else {
            return;
        };
        let binary = state.read().binary.clone();

        {
            let s = state.read();
            if s.is_loading_xrefs {
                return;
            }
        }
        {
            let mut s = state.write();
            s.is_loading_xrefs = true;
            s.current_xref_callers.clear();
            s.current_xref_callees.clear();
        }

        spawn(async move {
            let (callers, callees) = crate::engine::run_xrefs(binary, addr).await;

            let nc = callers.len();
            let nk = callees.len();
            let mut s = state.write();
            s.current_xref_callers = callers;
            s.current_xref_callees = callees;
            s.is_loading_xrefs = false;
            s.push_log(LogEntry::info(format!("Xrefs: {nc} callers, {nk} callees")));
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

    // ── Jump helper ──────────────────────────────────────────────────────────
    let mut jump_to = move |addr: u64, hint_name: Option<String>| {
        let binary = state.read().binary.clone();

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
                let fallback = hint_name.unwrap_or_else(|| format!("sub_{addr:x}"));
                (FunctionKind::Code, fallback)
            }
        };

        {
            let mut s = state.write();
            s.current_function_addr = Some(addr);
            s.current_function_kind = kind;
            s.decompiled_code = None;
            s.decompiled_nir = None;
            s.current_cfg = None;
            s.current_xref_callers.clear();
            s.current_xref_callees.clear();
            s.is_loading_xrefs = false;
            s.is_decompiling = true;
            s.navigate_to(addr);
            s.active_bottom_tab = BottomTab::Logs;
            s.push_log(LogEntry::info(format!(
                "Jumped to {resolved_name}  @  0x{addr:x}"
            )));
        }

        let name_for_task = resolved_name;
        spawn(async move {
            run_decompile(state, binary, addr, name_for_task).await;
        });
    };

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
                            let label = row.fn_name.clone()
                                .unwrap_or_else(|| format!("sub_{:x}", row.from_addr));
                            let addr_str = format!("0x{:x}", row.from_addr);
                            let kind_str = row.kind.as_str();
                            let jump_addr = row.from_addr;
                            let jump_name = row.fn_name.clone();
                            rsx! {
                                div {
                                    class: "xrefs-row xrefs-row-clickable",
                                    key: "{i}",
                                    onclick: move |_| jump_to(jump_addr, jump_name.clone()),
                                    span { class: "xrefs-kind", "{kind_str}" }
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
                                .map(str::to_string)
                                .unwrap_or_else(|| "unknown".to_string());
                            let addr_str = row.to_addr
                                .map(|a| format!("0x{:x}", a))
                                .unwrap_or_else(|| "indirect".to_string());
                            let kind_str = row.kind.as_str();
                            let jump_addr = row.to_addr;
                            let jump_name = row.fn_name.clone().or(row.symbol.clone());
                            rsx! {
                                div {
                                    class: if jump_addr.is_some() {
                                        "xrefs-row xrefs-row-clickable"
                                    } else {
                                        "xrefs-row"
                                    },
                                    key: "{i}",
                                    onclick: move |_| {
                                        if let Some(a) = jump_addr {
                                            jump_to(a, jump_name.clone());
                                        }
                                    },
                                    span { class: "xrefs-kind", "{kind_str}" }
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
