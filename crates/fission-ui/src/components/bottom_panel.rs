//! Bottom panel — tabbed area with Output log, CFG viewer, and Xrefs.

use crate::components::cfg_view::CfgView;
use crate::components::xrefs_view::XrefsView;
use crate::state::{use_app_state, BottomTab, LogLevel};
use dioxus::prelude::*;

#[component]
pub fn BottomPanel() -> Element {
    let mut state = use_app_state();
    let active_tab = state.read().active_bottom_tab.clone();

    let tab_cls = |t: &BottomTab| -> &'static str {
        if *t == active_tab { "bottom-tab is-active" } else { "bottom-tab" }
    };

    rsx! {
        div { class: "bottom-panel",
            // ── Tab bar ────────────────────────────────────────────────────
            div { class: "bottom-tabs",
                div {
                    class: tab_cls(&BottomTab::Logs),
                    onclick: move |_| state.write().active_bottom_tab = BottomTab::Logs,
                    "Output"
                }
                div {
                    class: tab_cls(&BottomTab::Cfg),
                    onclick: move |_| state.write().active_bottom_tab = BottomTab::Cfg,
                    "CFG"
                }
                div {
                    class: tab_cls(&BottomTab::Xrefs),
                    onclick: move |_| state.write().active_bottom_tab = BottomTab::Xrefs,
                    "Xrefs"
                }
                div { class: "tab-spacer" }
                // Clear is only meaningful for Logs tab
                if active_tab == BottomTab::Logs {
                    div {
                        class: "tab-action",
                        onclick: move |_| state.write().log_entries.clear(),
                        "Clear"
                    }
                }
            }

            // ── Content ────────────────────────────────────────────────────
            div { class: "bottom-content",
                match active_tab {
                    BottomTab::Logs => {
                        let entries: Vec<_> = state.read().log_entries.iter().cloned().collect();
                        rsx! {
                            div { class: "log-scroll",
                                for (i, entry) in entries.iter().enumerate() {
                                    {
                                        let (row_cls, level_str) = match entry.level {
                                            LogLevel::Info  => ("log-row lvl-info",  "INFO"),
                                            LogLevel::Warn  => ("log-row lvl-warn",  "WARN"),
                                            LogLevel::Error => ("log-row lvl-error", "ERROR"),
                                        };
                                        let ts_str = if entry.timestamp > 0 {
                                            let h = (entry.timestamp / 3600) % 24;
                                            let m = (entry.timestamp / 60) % 60;
                                            let s = entry.timestamp % 60;
                                            format!("{h:02}:{m:02}:{s:02}")
                                        } else {
                                            String::new()
                                        };
                                        rsx! {
                                            div { class: "{row_cls}", key: "{i}",
                                                div { class: "log-dot" }
                                                if !ts_str.is_empty() {
                                                    span { class: "log-ts", "{ts_str}" }
                                                }
                                                span { class: "log-level", "{level_str}" }
                                                span { class: "log-msg",   "{entry.message}" }
                                            }
                                        }
                                    }
                                }
                                // Anchor div: CSS scroll-snap scrolls here automatically
                                div { id: "log-bottom", class: "log-scroll-anchor" }
                            }
                        }
                    }
                    BottomTab::Cfg   => rsx! { CfgView {} },
                    BottomTab::Xrefs => rsx! { XrefsView {} },
                }
            }
        }
    }
}
