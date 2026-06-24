use dioxus::prelude::*;

#[component]
pub fn BottomPanel() -> Element {
    rsx! {
        div { class: "bottom-panel",
            div { class: "bottom-tabs",
                div { class: "tab active", "Logs" }
                div { class: "tab", "Timeline" }
                div { class: "tab", "CFG" }
            }
            div { class: "bottom-content",
                div { class: "log-entry info", "[INFO] Fission UI loaded successfully." }
                div { class: "log-entry info", "[INFO] Ready to load binary." }
            }
        }
    }
}
