use crate::engine::{
    batch_decompile_one, batch_decompile_one_with_facts, build_facts_blocking,
    load_binary_blocking, DecompileOutput, LoadResult,
};
use crate::state::{use_app_state, LogEntry};
use dioxus::prelude::*;
use std::sync::Arc;

// ── Inline SVG helpers ───────────────────────────────────────────────────────
// These avoid emoji entirely and keep icons crisp at any DPI.

fn svg_fission_mark() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "14",
            height: "14",
            view_box: "0 0 14 14",
            fill: "none",
            // Lightning bolt mark
            path {
                d: "M8.5 1.5 L3.5 8 H7 L5.5 12.5 L10.5 6 H7 Z",
                fill: "white",
                fill_rule: "evenodd",
            }
        }
    }
}

fn svg_folder() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "12",
            height: "12",
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.5",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M2 4.5A1.5 1.5 0 0 1 3.5 3h2.756a1.5 1.5 0 0 1 1.06.44l.5.5H12.5A1.5 1.5 0 0 1 14 5.5v6A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5v-7z" }
        }
    }
}

fn svg_nav_back() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "14", height: "14",
            view_box: "0 0 16 16",
            fill: "none", stroke: "currentColor",
            stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
            polyline { points: "10 3 5 8 10 13" }
        }
    }
}

fn svg_nav_forward() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "14", height: "14",
            view_box: "0 0 16 16",
            fill: "none", stroke: "currentColor",
            stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
            polyline { points: "6 3 11 8 6 13" }
        }
    }
}

#[component]
pub fn TitleBar() -> Element {
    let mut state = use_app_state();

    // ── Batch decompile coroutine ─────────────────────────────────────────────
    // Messages: () = start a batch run (cancel is toggled via state.batch_cancel).
    let batch_tx: Coroutine<()> = use_coroutine(move |mut rx: UnboundedReceiver<()>| async move {
        use futures_util::stream::StreamExt;
        while (rx.next().await).is_some() {
            // Collect snapshot of functions + binary at start time
            let (binary, functions) = {
                let s = state.read();
                (s.binary.clone(), s.functions.clone())
            };
            let Some(binary) = binary else {
                continue;
            };
            if functions.is_empty() {
                continue;
            }

            let total = functions.len();
            {
                let mut s = state.write();
                s.is_batch_running = true;
                s.batch_done = 0;
                s.batch_total = total;
                s.batch_cancel = false;
                s.push_log(LogEntry::info(format!(
                    "Batch decompile started: {total} functions (prebuilding facts…)"
                )));
            }

            // Prebuild FactStore ONCE so we don't rebuild static facts for every function
            let bin_for_facts = Arc::clone(&binary);
            let facts = tokio::task::spawn_blocking(move || {
                Arc::new(build_facts_blocking(bin_for_facts.as_ref()))
            })
            .await;

            let Ok(facts) = facts else {
                state.write().is_batch_running = false;
                state.write().push_log(LogEntry::error("Failed to build static facts for batch."));
                continue;
            };

            state.write().push_log(LogEntry::info("Static facts built. Running decompile pipeline…"));

            let mut ok_count = 0usize;
            let mut err_count = 0usize;
            let mut fb_count = 0usize;

            for fi in &functions {
                // Check cancellation flag before each item
                if state.read().batch_cancel {
                    state
                        .write()
                        .push_log(LogEntry::warn("Batch decompile cancelled."));
                    break;
                }

                let bin_clone = Arc::clone(&binary);
                let facts_clone = Arc::clone(&facts);
                let addr = fi.address;
                let name_clone = fi.name.clone();

                // Decompile on a blocking thread so the event loop stays live
                let result = tokio::task::spawn_blocking(move || {
                    batch_decompile_one_with_facts(&bin_clone, &facts_clone, addr, &name_clone)
                })
                .await;

                match result {
                    Ok(r) => {
                        if r.ok {
                            ok_count += 1;
                            if r.fell_back {
                                fb_count += 1;
                            }
                        } else {
                            err_count += 1;
                        }
                    }
                    Err(_) => {
                        err_count += 1;
                    }
                }

                state.write().batch_done += 1;
            }

            {
                let mut s = state.write();
                s.is_batch_running = false;
                s.push_log(LogEntry::info(format!(
                    "Batch complete: {ok_count} ok  {fb_count} fallback  {err_count} error  / {total} total"
                )));
            }
        }
    });

    let open_binary = move |_| {
        {
            let mut s = state.write();
            s.is_loading_binary = true;
            s.push_log(LogEntry::info("Opening file selector…"));
        }

        spawn(async move {
            // rfd::FileDialog is blocking; run off the async executor
            let picked = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .set_title("Select Binary")
                    .add_filter(
                        "Executables",
                        &["exe", "dll", "sys", "so", "dylib", "elf", "out", "bin"],
                    )
                    .add_filter("All files", &["*"])
                    .pick_file()
            })
            .await;

            let path = match picked {
                Ok(Some(p)) => p,
                Ok(None) => {
                    let mut s = state.write();
                    s.is_loading_binary = false;
                    s.push_log(LogEntry::info("Selection cancelled."));
                    return;
                }
                Err(e) => {
                    let mut s = state.write();
                    s.is_loading_binary = false;
                    s.push_log(LogEntry::error(format!("Dialog error: {e}")));
                    return;
                }
            };

            let display = path.display().to_string();
            state
                .write()
                .push_log(LogEntry::info(format!("Loading {display}")));

            let path_clone = path.clone();
            let result: Result<LoadResult, String> =
                tokio::task::spawn_blocking(move || load_binary_blocking(&path_clone))
                    .await
                    .unwrap_or_else(|e| Err(format!("Task join error: {e}")));

            match result {
                Ok(load) => {
                    let summary = load.summary.clone();
                    let fn_count = load.functions.len();
                    {
                        let mut s = state.write();
                        s.binary_name = Some(
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        );
                        s.binary = load.binary.clone();
                        s.functions = load.functions;
                        s.strings   = load.strings;
                        s.current_function_addr = None;
                        s.decompiled_code = None;
                        s.decompiled_nir = None;
                        s.sidebar_search = String::new();
                        s.rename_map.clear();
                        s.is_loading_binary = false;
                        s.push_log(LogEntry::info(format!("Loaded — {summary}")));
                        s.push_log(LogEntry::info(format!("{fn_count} functions discovered.")));
                    }
                }
                Err(e) => {
                    let mut s = state.write();
                    s.is_loading_binary = false;
                    s.push_log(LogEntry::error(format!("Load failed: {e}")));
                }
            }
        });
    };

    let binary_name = {
        let s = state.read();
        s.binary_name.clone()
    };

    let title_text = match binary_name.as_deref() {
        Some(name) => format!("Fission — {}", name),
        None => "Fission".to_string(),
    };

    let is_loading = state.read().is_loading_binary;

    let can_back = state.read().can_nav_back();
    let can_forward = state.read().can_nav_forward();

    rsx! {
        div { class: "title-bar",
            // ── Logo + nav arrows ─────────────────────────────────────────
            div { class: "title-logo",
                div { class: "logo-mark",
                    {svg_fission_mark()}
                }
                span { class: "logo-wordmark", "Fission" }

                // Back / Forward navigation buttons
                div { class: "nav-arrows",
                    button {
                        class: if can_back { "nav-btn" } else { "nav-btn disabled" },
                        title: "Go Back  (Cmd [)",
                        onclick: move |_| {
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
                                        crate::components::sidebar::run_decompile(
                                            state, Some(binary), None, addr, name
                                        ).await;
                                    });
                                }
                            }
                        },
                        {svg_nav_back()}
                    }
                    button {
                        class: if can_forward { "nav-btn" } else { "nav-btn disabled" },
                        title: "Go Forward  (Cmd ])",
                        onclick: move |_| {
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
                                        crate::components::sidebar::run_decompile(
                                            state, Some(binary), None, addr, name
                                        ).await;
                                    });
                                }
                            }
                        },
                        {svg_nav_forward()}
                    }
                }
            }

            // ── Menu bar ──────────────────────────────────────────────────
            div { class: "title-menu",
                // File
                div { class: "menu-item",
                    div { class: "menu-trigger", "File" }
                    div { class: "dropdown",
                        div {
                            class: if is_loading { "dropdown-item disabled" } else { "dropdown-item" },
                            onclick: open_binary,
                            div { class: "dropdown-item-label",
                                {svg_folder()}
                                span { "Open Binary…" }
                            }
                            span { class: "dropdown-item-kbd", "Cmd O" }
                        }
                        div { class: "dropdown-separator" }
                        div { class: "dropdown-item disabled",
                            div { class: "dropdown-item-label", "Preferences" }
                            span { class: "dropdown-item-kbd", "Cmd ," }
                        }
                    }
                }

                // View
                div { class: "menu-item",
                    div { class: "menu-trigger", "View" }
                    div { class: "dropdown",
                        div {
                            class: "dropdown-item",
                            onclick: move |_| state.write().toggle_sidebar(),
                            div { class: "dropdown-item-label", "Toggle Sidebar" }
                            span { class: "dropdown-item-kbd", "Cmd B" }
                        }
                        div {
                            class: "dropdown-item",
                            onclick: move |_| state.write().toggle_bottom_panel(),
                            div { class: "dropdown-item-label", "Toggle Log Panel" }
                            span { class: "dropdown-item-kbd", "Cmd J" }
                        }
                    }
                }

                // Analysis
                div { class: "menu-item",
                    div { class: "menu-trigger", "Analysis" }
                    div { class: "dropdown",
                        {
                            let has_binary = state.read().binary.is_some();
                            let is_running = state.read().is_batch_running;
                            let batch_done_val  = state.read().batch_done;
                            let batch_total_val = state.read().batch_total;
                            let item_cls = if !has_binary || is_running {
                                "dropdown-item disabled"
                            } else {
                                "dropdown-item"
                            };
                            rsx! {
                                div {
                                    class: "{item_cls}",
                                    onclick: move |_| {
                                        if state.read().binary.is_some() && !state.read().is_batch_running {
                                            batch_tx.send(());
                                        }
                                    },
                                    div { class: "dropdown-item-label",
                                        if is_running {
                                            span { "{batch_done_val} / {batch_total_val}  decompiled" }
                                        } else {
                                            span { "Analyse All Functions" }
                                        }
                                    }
                                }
                                if is_running {
                                    div {
                                        class: "dropdown-item",
                                        onclick: move |_| {
                                            state.write().batch_cancel = true;
                                        },
                                        div { class: "dropdown-item-label batch-cancel",
                                            span { "Cancel Batch" }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "dropdown-item disabled",
                            div { class: "dropdown-item-label", "Run FID Signatures" }
                        }
                    }
                }
            }

            // ── Binary name (centre) ───────────────────────────────────────
            div { class: "title-binary",
                if is_loading {
                    div { class: "loading-indicator",
                        div { class: "spinner" }
                        span { "Loading binary…" }
                    }
                } else if let Some(name) = binary_name {
                    span { class: "title-binary-name", "{name}" }
                } else {
                    span { "No binary loaded" }
                }
            }
        }
    }
}
