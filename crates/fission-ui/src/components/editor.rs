use crate::state::{use_app_state, EditorTab, FunctionKind};
use dioxus::prelude::*;

// ── Find-bar match highlight helper ─────────────────────────────────────────

/// Wrap occurrences of `needle` in `<mark class="find-match">…</mark>`.
/// Input is already HTML-escaped code, so we search on the raw string.
/// Returns the string unchanged if needle is empty.
fn apply_find_highlights(html: &str, needle: &str) -> String {
    if needle.is_empty() {
        return html.to_string();
    }
    let lower_html = html.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let nlen = lower_needle.len();
    let mut out = String::with_capacity(html.len() + 64);
    let mut pos = 0;
    while let Some(idx) = lower_html[pos..].find(&lower_needle) {
        let abs = pos + idx;
        out.push_str(&html[pos..abs]);
        out.push_str("<mark class=\"find-match\">");
        out.push_str(&html[abs..abs + nlen]);
        out.push_str("</mark>");
        pos = abs + nlen;
    }
    out.push_str(&html[pos..]);
    out
}

// ── SVG icons ────────────────────────────────────────────────────────────────

fn svg_code() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "26",
            height: "26",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.4",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "16 18 22 12 16 6" }
            polyline { points: "8 6 2 12 8 18" }
        }
    }
}

fn svg_hex() -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            width: "26",
            height: "26",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.4",
            stroke_linecap: "round",
            rect { x: "2", y: "3", width: "20", height: "18", rx: "2" }
            line { x1: "2", y1: "9", x2: "22", y2: "9" }
            line { x1: "9", y1: "21", x2: "9", y2: "9" }
        }
    }
}

// ── Syntax highlighting ──────────────────────────────────────────────────────
// One Dark Pro token colours via .tok-* CSS classes.

const C_KEYWORDS: &[&str] = &[
    "int", "long", "char", "void", "short", "unsigned", "signed", "float", "double", "bool",
    "struct", "union", "enum", "typedef", "return", "if", "else", "while", "for", "do", "break",
    "continue", "switch", "case", "default", "goto", "sizeof", "const", "static", "extern",
    "register", "volatile", "inline", "auto",
];

const C_TYPES: &[&str] = &[
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "size_t",
    "ssize_t",
    "ptrdiff_t",
    "uintptr_t",
    "intptr_t",
    "NULL",
    "true",
    "false",
    "BOOL",
    "DWORD",
    "WORD",
    "BYTE",
    "HANDLE",
    "LPVOID",
    "LPCSTR",
];

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Tokenise a single line and emit HTML spans.
fn highlight_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len() * 2);
    let mut rest = line;

    // Leading whitespace – preserve as-is (no wrap needed)
    let leading = rest.chars().take_while(|c| c.is_whitespace()).count();
    if leading > 0 {
        let ws = &rest[..leading];
        out.push_str(&html_escape(ws));
        rest = &rest[leading..];
    }

    // Full-line comment or preprocessor
    if rest.starts_with("//") {
        out.push_str("<span class=\"tok-cmt\">");
        out.push_str(&html_escape(rest));
        out.push_str("</span>");
        return out;
    }
    if rest.starts_with('#') {
        out.push_str("<span class=\"tok-pp\">");
        out.push_str(&html_escape(rest));
        out.push_str("</span>");
        return out;
    }

    while !rest.is_empty() {
        // Whitespace
        if rest.starts_with(|c: char| c.is_whitespace()) {
            let n = rest.chars().take_while(|c| c.is_whitespace()).count();
            out.push_str(&html_escape(&rest[..n]));
            rest = &rest[n..];
            continue;
        }

        // Inline comment
        if rest.starts_with("//") {
            out.push_str("<span class=\"tok-cmt\">");
            out.push_str(&html_escape(rest));
            out.push_str("</span>");
            rest = "";
            continue;
        }

        // String literal
        if rest.starts_with('"') {
            let end_inner = rest[1..].find('"').map(|i| i + 2).unwrap_or(rest.len());
            let tok = &rest[..end_inner];
            out.push_str("<span class=\"tok-str\">");
            out.push_str(&html_escape(tok));
            out.push_str("</span>");
            rest = &rest[end_inner..];
            continue;
        }

        // Char literal
        if rest.starts_with('\'') {
            let end_inner = rest[1..].find('\'').map(|i| i + 2).unwrap_or(rest.len());
            let tok = &rest[..end_inner];
            out.push_str("<span class=\"tok-str\">");
            out.push_str(&html_escape(tok));
            out.push_str("</span>");
            rest = &rest[end_inner..];
            continue;
        }

        // Number literal (hex, decimal, float)
        if rest.starts_with("0x") || rest.starts_with("0X") {
            let end = rest[2..]
                .find(|c: char| !c.is_ascii_hexdigit())
                .map(|i| i + 2)
                .unwrap_or(rest.len());
            out.push_str("<span class=\"tok-num\">");
            out.push_str(&html_escape(&rest[..end]));
            out.push_str("</span>");
            rest = &rest[end..];
            continue;
        }
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_')
                .unwrap_or(rest.len());
            out.push_str("<span class=\"tok-num\">");
            out.push_str(&html_escape(&rest[..end]));
            out.push_str("</span>");
            rest = &rest[end..];
            continue;
        }

        // Identifier: keyword / type / function call / plain
        if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            let tok = &rest[..end];
            let after = &rest[end..];

            let cls = if C_KEYWORDS.contains(&tok) {
                "tok-kw"
            } else if C_TYPES.contains(&tok) {
                "tok-ty"
            } else if after.trim_start().starts_with('(') {
                "tok-fn"
            } else if tok.starts_with("sub_") && tok.len() > 4 && tok[4..].chars().all(|c| c.is_ascii_hexdigit()) {
                "tok-addr"
            } else {
                ""
            };

            if cls.is_empty() {
                out.push_str(&html_escape(tok));
            } else {
                out.push_str(&format!("<span class=\"{cls}\">"));
                out.push_str(&html_escape(tok));
                out.push_str("</span>");
            }
            rest = after;
            continue;
        }

        // Single char (operator, punctuation)
        let char_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        let ch = &rest[..char_len];
        out.push_str(&html_escape(ch));
        rest = &rest[char_len..];
    }
    out
}

/// Returns `(gutter_html, code_html)` for `dangerous_inner_html` injection.
fn render_with_lines(source: &str) -> (String, String) {
    let lines: Vec<&str> = source.lines().collect();
    let count = lines.len();

    // Gutter: one number per line, newline-separated
    let gutter: String = (1..=count)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    // Code: highlighted lines, newline-separated
    let code: String = lines
        .iter()
        .map(|l| highlight_line(l))
        .collect::<Vec<_>>()
        .join("\n");

    (gutter, code)
}

/// Hex dump of `limit` bytes, formatted as `OFFSET  hex…  ascii`.
fn hex_dump(data: &[u8], limit: usize) -> String {
    let data = &data[..data.len().min(limit)];
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let offset = i * 16;
        let hex: String = chunk
            .iter()
            .enumerate()
            .map(|(j, b)| {
                if j == 8 {
                    format!(" {:02x}", b)
                } else {
                    format!("{:02x} ", b)
                }
            })
            .collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!("{offset:08x}  {hex:<49}  {ascii}\n"));
    }
    if data.len() == limit {
        out.push_str(&format!("\n[output truncated at {limit} bytes]\n"));
    }
    out
}

// ── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn Editor() -> Element {
    let mut state = use_app_state();

    let active_tab    = state.read().active_tab.clone();
    let is_decompiling = state.read().is_decompiling;
    let fn_name       = state.read().current_function_name();
    let fn_kind       = state.read().current_function_kind.clone();
    let find_open     = state.read().find_bar_open;
    let find_query    = state.read().find_query.clone();

    // Kind badge
    let kind_badge: Option<(&'static str, &'static str)> = match &fn_kind {
        FunctionKind::Import { .. } => Some(("kind-badge-imp",   "IMPORT STUB")),
        FunctionKind::Thunk { .. }  => Some(("kind-badge-thunk", "THUNK")),
        FunctionKind::Code          => None,
    };

    let tab_cls = |tab: &EditorTab| -> &'static str {
        if *tab == active_tab { "tab is-active" } else { "tab" }
    };

    let is_import_stub = matches!(fn_kind, FunctionKind::Import { .. });
    let is_thunk       = matches!(fn_kind, FunctionKind::Thunk { .. });

    // ── Body ─────────────────────────────────────────────────────────────────
    let body: Element = if is_decompiling {
        rsx! {
            div { class: "editor-decompiling",
                div { class: "spinner spinner-lg" }
                span { class: "decompiling-label", "Decompiling…" }
            }
        }
    } else {
        match &active_tab {
            EditorTab::Pseudocode | EditorTab::Nir => {
                let code_str = state.read().editor_code().map(str::to_string);
                if let Some(code) = code_str {
                    if is_import_stub {
                        rsx! {
                            div { class: "import-info-panel",
                                pre { class: "import-info-body", "{code}" }
                            }
                        }
                    } else {
                        let (gutter, raw_highlighted) = render_with_lines(&code);
                        // Apply find-bar highlights on top of syntax highlighting
                        let highlighted = apply_find_highlights(&raw_highlighted, &find_query);
                        rsx! {
                            if is_thunk {
                                div { class: "kind-notice kind-notice-thunk",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        width: "14", height: "14",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                        line { x1: "12", y1: "9", x2: "12", y2: "13" }
                                        line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                                    }
                                    span {
                                        "Import thunk — output appears self-recursive because the "
                                        "jump target carries the same IAT symbol name as this function."
                                    }
                                }
                            }
                            div { class: "code-layout",
                                pre {
                                    class: "line-gutter",
                                    aria_hidden: "true",
                                    dangerous_inner_html: "{gutter}"
                                }
                                div { class: "code-region",
                                    code { dangerous_inner_html: "{highlighted}" }
                                }
                            }
                        }
                    }
                } else {
                    let msg = if state.read().binary.is_some() {
                        "Select a function from the sidebar to decompile."
                    } else {
                        "Open a binary to begin."
                    };
                    rsx! {
                        div { class: "editor-placeholder",
                            div { class: "placeholder-icon-wrap", {svg_code()} }
                            span { class: "placeholder-title", "Nothing to show" }
                            span { class: "placeholder-sub", "{msg}" }
                        }
                    }
                }
            }
            EditorTab::Hex => {
                // Prefer the current function's byte range; fall back to file header.
                let dump = {
                    let s = state.read();
                    if let (Some(binary), Some(fn_addr)) = (&s.binary, s.current_function_addr) {
                        // Find function in the list to get its size
                        let fn_info = s.functions.iter().find(|f| f.address == fn_addr);
                        if let Some(fi) = fn_info {
                            // Convert VA to file offset via sections
                            let maybe_fo = binary.sections.iter().find_map(|sec| {
                                if fn_addr >= sec.virtual_address
                                    && fn_addr < sec.virtual_address + sec.virtual_size
                                {
                                    let offset_in_sec = fn_addr - sec.virtual_address;
                                    Some(sec.file_offset + offset_in_sec)
                                } else {
                                    None
                                }
                            });
                            if let Some(fo) = maybe_fo {
                                let fo = fo as usize;
                                let sz = if fi.size > 0 { fi.size as usize } else { 256 };
                                let end = (fo + sz).min(binary.data.as_slice().len());
                                if fo < binary.data.as_slice().len() {
                                    Some((fn_addr, hex_dump(&binary.data.as_slice()[fo..end], sz)))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                    .or_else(|| {
                        s.binary.as_ref().map(|b| (0u64, hex_dump(b.data.as_slice(), 4096)))
                    })
                };
                if let Some((_base_va, hex)) = dump {
                    let line_count = hex.lines().count();
                    let gutter_str: String = (1..=line_count)
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    rsx! {
                        div { class: "code-layout hex-layout",
                            pre {
                                class: "line-gutter",
                                aria_hidden: "true",
                                "{gutter_str}"
                            }
                            div { class: "code-region",
                                code { "{hex}" }
                            }
                        }
                    }
                } else {
                    rsx! {
                        div { class: "editor-placeholder",
                            div { class: "placeholder-icon-wrap", {svg_hex()} }
                            span { class: "placeholder-title", "No binary loaded" }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "editor-container",
            // ── Tab bar ───────────────────────────────────────────────────
            div { class: "editor-tabs",
                // Breadcrumb
                div { class: "breadcrumb",
                    span { class: "breadcrumb-sep", "fission" }
                    if let Some(name) = fn_name {
                        span { class: "breadcrumb-sep", "/" }
                        span { class: "breadcrumb-fn", "{name}" }
                    }
                    // Kind badge
                    if let Some((badge_cls, badge_label)) = kind_badge {
                        span { class: "breadcrumb-kind {badge_cls}", "{badge_label}" }
                    }
                }

                div { class: "tab-group",
                    div {
                        class: tab_cls(&EditorTab::Pseudocode),
                        onclick: move |_| state.write().active_tab = EditorTab::Pseudocode,
                        "Pseudocode"
                    }
                    div {
                        class: tab_cls(&EditorTab::Nir),
                        onclick: move |_| state.write().active_tab = EditorTab::Nir,
                        "NIR"
                    }
                    div {
                        class: tab_cls(&EditorTab::Hex),
                        onclick: move |_| state.write().active_tab = EditorTab::Hex,
                        "Hex"
                    }
                }
            }

            // ── Body ──────────────────────────────────────────────────────
            div { class: "editor-body",
                {body}
            }

            // ── Find bar (Cmd+F / Ctrl+F) ─────────────────────────────────
            if find_open {
                div { class: "find-bar",
                    span { class: "find-bar-label", "Find" }
                    input {
                        class: "find-bar-input",
                        r#type: "text",
                        placeholder: "Search in file…",
                        value: "{find_query}",
                        autofocus: true,
                        oninput: move |e| state.write().find_query = e.value().clone(),
                        onkeydown: move |e| {
                            if e.key() == Key::Escape {
                                let mut s = state.write();
                                s.find_bar_open = false;
                                s.find_query.clear();
                            }
                        },
                    }
                    if !find_query.is_empty() {
                        span { class: "find-bar-hint",
                            "Escape to close"
                        }
                    }
                    button {
                        class: "find-bar-close",
                        onclick: move |_| {
                            let mut s = state.write();
                            s.find_bar_open = false;
                            s.find_query.clear();
                        },
                        "x"
                    }
                }
            }
        }
    }
}
