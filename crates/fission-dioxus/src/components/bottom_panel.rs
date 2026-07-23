use crate::state::{BottomTab, LogLevel, use_app_state};
use dioxus::prelude::*;

#[component]
pub fn BottomPanel() -> Element {
    let mut state = use_app_state();
    let active_tab = state.read().active_bottom_tab.clone();

    let log_cls = |t: &BottomTab| -> &'static str {
        if *t == active_tab { "bottom-tab is-active" } else { "bottom-tab" }
    };

    rsx! {
        div { class: "bottom-panel",
            // ── Tab bar ──────────────────────────────────────────────────────
            div { class: "bottom-tabs",
                div {
                    class: log_cls(&BottomTab::Logs),
                    onclick: move |_| state.write().active_bottom_tab = BottomTab::Logs,
                    "Output"
                }
                div {
                    class: log_cls(&BottomTab::Cfg),
                    onclick: move |_| state.write().active_bottom_tab = BottomTab::Cfg,
                    "CFG"
                }

                div { class: "tab-spacer" }

                // Clear button
                div {
                    class: "tab-action",
                    onclick: move |_| state.write().log_entries.clear(),
                    "Clear"
                }
            }

            // ── Content ───────────────────────────────────────────────────────
            div { class: "bottom-content",
                match active_tab {
                    BottomTab::Logs => {
                        let entries: Vec<_> = state.read().log_entries.iter().cloned().collect();
                        rsx! {
                            for (i, entry) in entries.iter().enumerate() {
                                {
                                    let (row_cls, level_str, dot_cls) = match entry.level {
                                        LogLevel::Info  => ("log-row lvl-info",  "INFO",  "log-dot"),
                                        LogLevel::Warn  => ("log-row lvl-warn",  "WARN",  "log-dot"),
                                        LogLevel::Error => ("log-row lvl-error", "ERROR", "log-dot"),
                                    };
                                    rsx! {
                                        div { class: "{row_cls}", key: "{i}",
                                            div { class: "{dot_cls}" }
                                            span { class: "log-level", "{level_str}" }
                                            span { class: "log-msg",   "{entry.message}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    BottomTab::Cfg => rsx! {
                        div { class: "cfg-empty",
                            span { class: "cfg-empty-title", "CFG Viewer" }
                            span { class: "cfg-empty-sub", "Select a function to visualise its control-flow graph." }
                        }
                    },
                }
            }
        }
    }
}
