use dioxus::prelude::*;

#[component]
pub fn Sidebar() -> Element {
    rsx! {
        div { class: "sidebar",
            div { class: "sidebar-header", "Functions" }
            div { class: "sidebar-content",
                ul { class: "function-list",
                    li { class: "function-item active", "main (0x1000)" }
                    li { class: "function-item", "sub_1040 (0x1040)" }
                    li { class: "function-item", "sub_1080 (0x1080)" }
                    li { class: "function-item", "printf (0x2000)" }
                }
            }
        }
    }
}
