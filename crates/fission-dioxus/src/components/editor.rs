use dioxus::prelude::*;

#[component]
pub fn Editor() -> Element {
    rsx! {
        div { class: "editor-container",
            div { class: "editor-tabs",
                div { class: "tab active", "Pseudocode" }
                div { class: "tab", "Assembly" }
                div { class: "tab", "Hex View" }
            }
            div { class: "editor-content",
                pre { class: "code-block",
                    code {
                        "int main(int argc, char** argv) {{\n"
                        "    printf(\"Hello, Fission Dioxus!\\n\");\n"
                        "    return 0;\n"
                        "}}"
                    }
                }
            }
        }
    }
}
