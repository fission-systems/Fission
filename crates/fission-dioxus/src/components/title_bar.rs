use crate::engine::{DecompileOutput, LoadResult, load_binary_blocking};
use crate::state::{LogEntry, use_app_state};
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

// ── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn TitleBar() -> Element {
    let mut state = use_app_state();

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
            state.write().push_log(LogEntry::info(format!("Loading {display}")));

            let result: Result<LoadResult, String> =
                tokio::task::spawn_blocking(move || load_binary_blocking(&path))
                    .await
                    .unwrap_or_else(|e| Err(format!("Task join error: {e}")));

            match result {
                Ok(load) => {
                    let summary = load.summary.clone();
                    let fn_count = load.functions.len();
                    {
                        let mut s = state.write();
                        s.loaded_binary_path =
                            Some(std::path::PathBuf::from(&display));
                        s.binary = Some(Arc::clone(&load.binary));
                        s.functions = load.functions;
                        s.current_function_addr = None;
                        s.decompiled_code = None;
                        s.decompiled_nir = None;
                        s.sidebar_search = String::new();
                        s.is_loading_binary = false;
                        s.push_log(LogEntry::info(format!("Loaded — {summary}")));
                        s.push_log(LogEntry::info(format!(
                            "{fn_count} functions discovered."
                        )));
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
        s.loaded_binary_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
    };

    let is_loading = state.read().is_loading_binary;

    rsx! {
        div { class: "title-bar",
            // ── Logo ──────────────────────────────────────────────────────
            div { class: "title-logo",
                div { class: "logo-mark",
                    {svg_fission_mark()}
                }
                span { class: "logo-wordmark", "Fission" }
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
                        div { class: "dropdown-item disabled",
                            div { class: "dropdown-item-label", "Toggle Sidebar" }
                            span { class: "dropdown-item-kbd", "Cmd B" }
                        }
                        div { class: "dropdown-item disabled",
                            div { class: "dropdown-item-label", "Toggle Log Panel" }
                            span { class: "dropdown-item-kbd", "Cmd J" }
                        }
                    }
                }

                // Analysis
                div { class: "menu-item",
                    div { class: "menu-trigger", "Analysis" }
                    div { class: "dropdown",
                        div { class: "dropdown-item disabled",
                            div { class: "dropdown-item-label", "Analyse All Functions" }
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
