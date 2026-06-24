#![allow(non_snake_case)]
#![allow(unused)]

use dioxus::prelude::*;
use tracing::Level;

mod components;
mod state;

use components::bottom_panel::BottomPanel;
use components::editor::Editor;
use components::sidebar::Sidebar;

const STYLE: Asset = asset!("assets/style.css");

fn main() {
    // Init logger
    dioxus_logger::init(Level::INFO).expect("failed to init logger");

    let cfg = dioxus::desktop::Config::new()
        .with_custom_head(r#"<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">"#.to_string());

    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    // Initialize global state context if needed here
    // ...

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE }

        div { class: "app-container",
            // Top Bar
            div { class: "title-bar",
                div { class: "title-logo", "Fission" }
                div { class: "title-menu",
                    span { "File" }
                    span { "Edit" }
                    span { "View" }
                    span { "Analysis" }
                    span { "Help" }
                }
            }

            // Main Content Area
            div { class: "main-workspace",
                Sidebar {}

                div { class: "editor-area",
                    Editor {}
                    BottomPanel {}
                }
            }

            // Status Bar
            div { class: "status-bar",
                span { "Ready" }
                span { class: "status-right", "x86_64 | Auto-Analysis Idle" }
            }
        }
    }
}
