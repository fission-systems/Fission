use crate::state::{use_app_state, EditorTab, FunctionKind, LogEntry};
use dioxus::prelude::*;

// ── Find-bar match highlight helper ─────────────────────────────────────────

/// Wrap occurrences of `needle` in `<mark class="find-match">…</mark>`; the
/// occurrence at `current_index` (0-based, wrapped to the match count) also
/// gets `id="find-match-current"` and an extra class, so the caller can
/// scroll it into view. Returns `(highlighted_html, match_count)`.
fn apply_find_highlights(html: &str, needle: &str, current_index: usize) -> (String, usize) {
    if needle.is_empty() {
        return (html.to_string(), 0);
    }
    let lower_html = html.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let nlen = lower_needle.len();
    let mut out = String::with_capacity(html.len() + 64);
    let mut pos = 0;
    let mut count = 0usize;
    while let Some(idx) = lower_html[pos..].find(&lower_needle) {
        let abs = pos + idx;
        out.push_str(&html[pos..abs]);
        if count == current_index {
            out.push_str("<mark id=\"find-match-current\" class=\"find-match find-match-current\">");
        } else {
            out.push_str("<mark class=\"find-match\">");
        }
        out.push_str(&html[abs..abs + nlen]);
        out.push_str("</mark>");
        pos = abs + nlen;
        count += 1;
    }
    out.push_str(&html[pos..]);
    (out, count)
}

/// Count case-insensitive occurrences of `needle` without building any HTML
/// -- used by the find bar's next/prev handlers to compute wraparound
/// without paying for a full highlight pass.
fn count_matches(html: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let lower_html = html.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let nlen = lower_needle.len();
    let mut pos = 0;
    let mut count = 0usize;
    while let Some(idx) = lower_html[pos..].find(&lower_needle) {
        pos += idx + nlen;
        count += 1;
    }
    count
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

// ── Save pseudocode to file ─────────────────────────────────────────────────
// Native only: rfd's file dialog isn't wired for wasm32 here, so the wasm32
// impl renders nothing rather than promise a save that can't happen (same
// lesson as the clipboard bug this session already fixed: a button must not
// claim to do something it can't actually do on the current target).

#[cfg(not(target_arch = "wasm32"))]
fn save_to_file_button(mut state: Signal<crate::state::AppState>) -> Element {
    let has_code = state.read().editor_code().is_some();
    let is_decompiling = state.read().is_decompiling;
    if !has_code || is_decompiling {
        return rsx! {};
    }
    rsx! {
        button {
            class: "tab-action-btn",
            title: "Save pseudocode to file…",
            onclick: move |_| {
                let code = state.read().editor_code().map(str::to_string);
                let Some(code) = code else { return; };
                let default_name = state.read().current_function_name()
                    .map(|n| format!("{n}.c"))
                    .unwrap_or_else(|| "pseudocode.c".to_string());
                spawn(async move {
                    let picked = rfd::AsyncFileDialog::new()
                        .set_title("Save Pseudocode")
                        .set_file_name(&default_name)
                        .add_filter("C source", &["c"])
                        .save_file()
                        .await;
                    let Some(handle) = picked else { return; };
                    let path = handle.path().to_path_buf();
                    let write_result = tokio::task::spawn_blocking({
                        let path = path.clone();
                        move || std::fs::write(path, code)
                    }).await;
                    match write_result {
                        Ok(Ok(())) => state.write().push_log(
                            crate::state::LogEntry::info(format!("Saved to {}", path.display()))
                        ),
                        Ok(Err(e)) => state.write().push_log(
                            crate::state::LogEntry::error(format!("Save failed: {e}"))
                        ),
                        Err(e) => state.write().push_log(
                            crate::state::LogEntry::error(format!("Save failed: {e}"))
                        ),
                    }
                });
            },
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                width: "13", height: "13",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z" }
                path { d: "M17 21v-8H7v8" }
                path { d: "M7 3v5h8" }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn save_to_file_button(_state: Signal<crate::state::AppState>) -> Element {
    rsx! {}
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
            } else if cls == "tok-addr" {
                // tok-addr: emit data-addr so JS-free Dioxus onClick can read it
                let addr_hex = &tok[4..]; // strip "sub_"
                out.push_str(&format!("<span class=\"{cls}\" data-addr=\"{addr_hex}\">"));
                out.push_str(&html_escape(tok));
                out.push_str("</span>");
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

/// Hex dump of `limit` bytes, formatted as `ADDRESS  hex…  ascii`, addresses
/// shown as `base_addr + row_offset` (real VA, not window-relative) so the
/// view can be correlated with pseudocode/xref addresses. Returns escaped
/// HTML (for `dangerous_inner_html`); when `highlight_addr` falls inside a
/// row, that row is wrapped in `<mark id="hex-target-row">` so the caller
/// can scroll it into view.
fn hex_dump(data: &[u8], limit: usize, base_addr: u64, highlight_addr: Option<u64>) -> String {
    let data = &data[..data.len().min(limit)];
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let row_addr = base_addr + (i * 16) as u64;
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
        let row = html_escape(&format!("{row_addr:016x}  {hex:<49}  {ascii}"));
        let is_target = highlight_addr.is_some_and(|h| h >= row_addr && h < row_addr + chunk.len() as u64);
        if is_target {
            out.push_str("<mark id=\"hex-target-row\" class=\"hex-highlight-row\">");
            out.push_str(&row);
            out.push_str("</mark>\n");
        } else {
            out.push_str(&row);
            out.push('\n');
        }
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

    // ── Click-to-navigate for `sub_XXXX` references in pseudocode ────────────
    // The highlighter (`highlight_line`, below) emits `data-addr` on each
    // `tok-addr` span, but that content is injected via `dangerous_inner_html`
    // (not part of the VDOM), so a plain Dioxus `onclick` on the wrapping
    // element can't identify which span was clicked. Delegate at the document
    // level via JS instead (`dioxus.send`/`Eval::recv` -- the standard,
    // platform-agnostic bridge, same on desktop and web) and navigate in Rust
    // when a `.tok-addr` element is clicked. `use_hook` guarantees this runs
    // exactly once for the component's lifetime, so the listener is never
    // registered twice even though `Editor` re-renders on every keystroke.
    use_hook(|| {
        spawn(async move {
            let mut eval = document::eval(
                r#"
                document.addEventListener('click', (e) => {
                    const el = e.target.closest && e.target.closest('.tok-addr');
                    if (el) {
                        const addr = el.getAttribute('data-addr');
                        if (addr) { dioxus.send(addr); }
                    }
                });
                "#,
            );
            loop {
                let Ok(addr_hex) = eval.recv::<String>().await else {
                    break;
                };
                let Ok(addr) = u64::from_str_radix(&addr_hex, 16) else {
                    continue;
                };
                crate::components::sidebar::navigate_to_address(state, addr).await;
            }
        });
    });

    // Inline comment-edit state for the Disasm view (mirrors sidebar.rs's
    // rename_addr/rename_draft pattern: only one row editable at a time).
    let mut comment_edit_addr: Signal<Option<u64>> = use_signal(|| None);
    let mut comment_edit_draft: Signal<String> = use_signal(String::new);

    let active_tab    = state.read().active_tab.clone();
    let is_decompiling = state.read().is_decompiling;
    let fn_name       = state.read().current_function_name();
    let fn_kind       = state.read().current_function_kind.clone();
    let find_open     = state.read().find_bar_open;
    let find_query    = state.read().find_query.clone();

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
                        let find_idx = state.read().find_current_index;
                        let (highlighted, _match_count) = apply_find_highlights(&raw_highlighted, &find_query, find_idx);
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
                    let msg = if state.read().has_binary_loaded() {
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
            EditorTab::Disasm => {
                let fn_addr = state.read().current_function_addr;
                let is_loading = state.read().is_loading_disasm;

                // Load on mount / whenever the selected function changes
                // (mirrors xrefs_view.rs's load-on-mount effect).
                use_effect(move || {
                    let Some(addr) = fn_addr else { return; };
                    {
                        let s = state.read();
                        if s.is_loading_disasm { return; }
                    }
                    let binary = state.read().binary.clone();
                    let session = state.read().server_session_id.clone();
                    {
                        let mut s = state.write();
                        s.is_loading_disasm = true;
                        s.current_disasm.clear();
                    }
                    spawn(async move {
                        let result = crate::engine::run_disasm(binary, session, addr).await;
                        let mut s = state.write();
                        match result {
                            Ok(rows) => s.current_disasm = rows,
                            Err(e) => s.push_log(LogEntry::error(format!("Disassembly failed: {e}"))),
                        }
                        s.is_loading_disasm = false;
                    });
                });

                if fn_addr.is_none() {
                    rsx! {
                        div { class: "editor-placeholder",
                            div { class: "placeholder-icon-wrap", {svg_code()} }
                            span { class: "placeholder-title", "Nothing to show" }
                            span { class: "placeholder-sub",
                                if state.read().has_binary_loaded() {
                                    "Select a function from the sidebar to disassemble."
                                } else {
                                    "Open a binary to begin."
                                }
                            }
                        }
                    }
                } else if is_loading {
                    rsx! {
                        div { class: "editor-decompiling",
                            div { class: "spinner spinner-lg" }
                            span { class: "decompiling-label", "Disassembling…" }
                        }
                    }
                } else {
                    let rows = state.read().current_disasm.clone();
                    if rows.is_empty() {
                        rsx! {
                            div { class: "editor-placeholder",
                                div { class: "placeholder-icon-wrap", {svg_code()} }
                                span { class: "placeholder-title", "No disassembly" }
                            }
                        }
                    } else {
                        let editing_addr = *comment_edit_addr.read();
                        rsx! {
                            div { class: "disasm-view",
                                for (i, row) in rows.iter().enumerate() {
                                    {
                                        let addr = row.address;
                                        let addr_str = format!("{:016x}", addr);
                                        let bytes_str = row.bytes_hex.clone();
                                        let text = row.text.clone();
                                        let target = row.target_addr;
                                        let is_editing = editing_addr == Some(addr);
                                        let comment = state.read().comments.get(&addr).cloned().unwrap_or_default();
                                        rsx! {
                                            div {
                                                class: if target.is_some() { "disasm-row disasm-row-clickable" } else { "disasm-row" },
                                                key: "{i}",
                                                onclick: move |_| {
                                                    if let Some(t) = target {
                                                        spawn(async move {
                                                            crate::components::sidebar::navigate_to_address(state, t).await;
                                                        });
                                                    }
                                                },
                                                span { class: "disasm-addr", "{addr_str}" }
                                                span { class: "disasm-bytes", "{bytes_str}" }
                                                span { class: "disasm-text", "{text}" }
                                                if is_editing {
                                                    input {
                                                        class: "disasm-comment-input",
                                                        r#type: "text",
                                                        value: "{comment_edit_draft}",
                                                        autofocus: true,
                                                        onclick: move |e| e.stop_propagation(),
                                                        oninput: move |ev| comment_edit_draft.set(ev.value().clone()),
                                                        onkeydown: move |ev| {
                                                            match ev.key() {
                                                                Key::Enter => {
                                                                    let text = comment_edit_draft.read().clone();
                                                                    if text.is_empty() {
                                                                        state.write().comments.remove(&addr);
                                                                    } else {
                                                                        state.write().comments.insert(addr, text);
                                                                    }
                                                                    comment_edit_addr.set(None);
                                                                }
                                                                Key::Escape => { comment_edit_addr.set(None); }
                                                                _ => {}
                                                            }
                                                        },
                                                        onblur: move |_| { comment_edit_addr.set(None); },
                                                    }
                                                } else {
                                                    span {
                                                        class: "disasm-comment",
                                                        title: "Double-click to edit comment",
                                                        ondoubleclick: move |e| {
                                                            e.stop_propagation();
                                                            comment_edit_draft.set(comment.clone());
                                                            comment_edit_addr.set(Some(addr));
                                                        },
                                                        "{comment}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            EditorTab::Hex => {
                let hex_view_target = state.read().hex_view_target;

                // Scroll the highlighted row into view whenever a new string
                // target is picked (mirrors sidebar's scrollIntoView pattern).
                use_effect(move || {
                    if hex_view_target.is_some() {
                        document::eval(
                            "document.getElementById('hex-target-row')?.scrollIntoView({block: 'center'});",
                        );
                    }
                });

                // Priority: string-click target > current function's bytes > file header.
                let hex_info: Option<(u64, u64, String, bool)> = {
                    let s = state.read();
                    hex_view_target.and_then(|target_va| {
                        let binary = s.binary.as_ref()?;
                        let file_off = binary.sections.iter().find_map(|sec| {
                            if target_va >= sec.virtual_address
                                && target_va < sec.virtual_address + sec.virtual_size
                            {
                                Some(sec.file_offset + (target_va - sec.virtual_address))
                            } else {
                                None
                            }
                        })?;
                        const WINDOW: u64 = 512;
                        const LEAD: u64 = 128;
                        let window_start_va = target_va.saturating_sub(LEAD) & !0xF;
                        let lead = target_va - window_start_va;
                        let fo_start = (file_off.saturating_sub(lead) as usize).min(binary.data.as_slice().len());
                        let end = (fo_start + WINDOW as usize).min(binary.data.as_slice().len());
                        if fo_start >= binary.data.as_slice().len() {
                            return None;
                        }
                        let hex = hex_dump(&binary.data.as_slice()[fo_start..end], WINDOW as usize, window_start_va, Some(target_va));
                        Some((window_start_va, (end - fo_start) as u64, hex, true))
                    })
                    .or_else(|| {
                        if let (Some(binary), Some(fn_addr)) = (&s.binary, s.current_function_addr) {
                            let fn_info = s.functions.iter().find(|f| f.address == fn_addr);
                            if let Some(fi) = fn_info {
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
                                        Some((fn_addr, sz as u64, hex_dump(&binary.data.as_slice()[fo..end], sz, fn_addr, None), false))
                                    } else { None }
                                } else { None }
                            } else { None }
                        } else { None }
                    })
                    .or_else(|| {
                        s.binary.as_ref().map(|b| (0u64, 4096u64, hex_dump(b.data.as_slice(), 4096, 0, None), false))
                    })
                };
                if let Some((base_va, sz_bytes, hex, is_target_view)) = hex_info {
                    let line_count = hex.lines().count();
                    let gutter_str: String = (1..=line_count)
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let fn_label = {
                        let s = state.read();
                        if is_target_view {
                            format!("String reference  |  VA 0x{:x}  |  showing {sz_bytes} bytes", hex_view_target.unwrap_or(base_va))
                        } else if let Some(addr) = s.current_function_addr {
                            let name = s.display_name(addr);
                            format!("{name}  |  VA 0x{base_va:x}  |  {sz_bytes} bytes")
                        } else {
                            format!("File header  |  {sz_bytes} bytes")
                        }
                    };
                    rsx! {
                        div { class: "hex-info-banner", "{fn_label}" }
                        div { class: "code-layout hex-layout",
                            pre {
                                class: "line-gutter",
                                aria_hidden: "true",
                                "{gutter_str}"
                            }
                            div { class: "code-region",
                                code { dangerous_inner_html: "{hex}" }
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
                        class: tab_cls(&EditorTab::Disasm),
                        onclick: move |_| state.write().active_tab = EditorTab::Disasm,
                        "Disasm"
                    }
                    div {
                        class: tab_cls(&EditorTab::Hex),
                        onclick: move |_| state.write().active_tab = EditorTab::Hex,
                        "Hex"
                    }
                }

                div { class: "tab-actions",
                    if matches!(active_tab, EditorTab::Pseudocode | EditorTab::Nir) {
                        if state.read().editor_code().is_some() && !is_decompiling {
                            button {
                                class: "tab-action-btn",
                                title: "Copy pseudocode to clipboard",
                                onclick: move |_| {
                                    let code = state.read().editor_code().map(str::to_string);
                                    let Some(code) = code else { return; };
                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        let mut s = state.write();
                                        match arboard::Clipboard::new().and_then(|mut ctx| ctx.set_text(&code)) {
                                            Ok(()) => s.push_log(crate::state::LogEntry::info("Pseudocode copied to clipboard.")),
                                            Err(e) => s.push_log(crate::state::LogEntry::error(format!("Clipboard copy failed: {e}"))),
                                        }
                                    }
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        // No arboard on wasm32 -- use the browser's
                                        // async Clipboard API via a fire-and-forget
                                        // JS eval instead (navigator.clipboard.writeText
                                        // returns a Promise; nothing here awaits it, so
                                        // no success/failure log is emitted -- previously
                                        // this path logged "copied" unconditionally
                                        // without ever actually writing anything).
                                        let escaped = code
                                            .replace('\\', "\\\\")
                                            .replace('`', "\\`")
                                            .replace('$', "\\$");
                                        document::eval(&format!(
                                            "navigator.clipboard.writeText(`{escaped}`);"
                                        ));
                                        state.write().push_log(crate::state::LogEntry::info("Pseudocode copied to clipboard."));
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "13", height: "13",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "1.8",
                                    stroke_linecap: "round",
                                    rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                                    path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                                }
                            }
                            {save_to_file_button(state)}
                        }
                    }
                }
            }

            // ── Body ──────────────────────────────────────────────────────
            div { class: "editor-body",
                {body}
            }

            // ── Find bar (Cmd+F / Ctrl+F) ─────────────────────────────────
            if find_open {
                {
                    let code_for_count = state.read().editor_code().map(str::to_string);
                    let total = code_for_count.as_ref().map(|code| {
                        let (_, raw) = render_with_lines(code);
                        count_matches(&raw, &find_query)
                    }).unwrap_or(0);
                    let find_idx_now = state.read().find_current_index;

                    // Scroll the current match into view whenever the query or
                    // index changes (mirrors the hex-target-row pattern).
                    {
                        let find_query_for_effect = find_query.clone();
                        use_effect(move || {
                            let _ = (find_query_for_effect.clone(), find_idx_now);
                            if total > 0 {
                                document::eval(
                                    "document.getElementById('find-match-current')?.scrollIntoView({block: 'center'});",
                                );
                            }
                        });
                    }

                    let mut go_next = move || {
                        let mut s = state.write();
                        if total > 0 {
                            s.find_current_index = (s.find_current_index + 1) % total;
                        }
                    };
                    let mut go_prev = move || {
                        let mut s = state.write();
                        if total > 0 {
                            s.find_current_index = (s.find_current_index + total - 1) % total;
                        }
                    };
                    rsx! {
                        div { class: "find-bar",
                            input {
                                class: "find-input",
                                r#type: "text",
                                placeholder: "Search in file…",
                                value: "{find_query}",
                                autofocus: true,
                                oninput: move |e| {
                                    let mut s = state.write();
                                    s.find_query = e.value().clone();
                                    s.find_current_index = 0;
                                },
                                onkeydown: move |e| {
                                    match e.key() {
                                        Key::Escape => {
                                            let mut s = state.write();
                                            s.find_bar_open = false;
                                            s.find_query.clear();
                                            s.find_current_index = 0;
                                        }
                                        Key::Enter if e.modifiers().shift() => {
                                            e.prevent_default();
                                            go_prev();
                                        }
                                        Key::Enter => {
                                            e.prevent_default();
                                            go_next();
                                        }
                                        _ => {}
                                    }
                                },
                            }
                            span { class: "find-match-count",
                                if find_query.is_empty() { "" }
                                else if total == 0 { "No matches" }
                                else { "{find_idx_now + 1} / {total}" }
                            }
                            button {
                                class: "find-bar-nav",
                                title: "Previous match  (Shift+Enter)",
                                disabled: total == 0,
                                onclick: move |_| go_prev(),
                                "\u{2191}"
                            }
                            button {
                                class: "find-bar-nav",
                                title: "Next match  (Enter)",
                                disabled: total == 0,
                                onclick: move |_| go_next(),
                                "\u{2193}"
                            }
                            button {
                                class: "find-bar-close",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.find_bar_open = false;
                                    s.find_query.clear();
                                    s.find_current_index = 0;
                                },
                                "×"
                            }
                        }
                    }
                }
            }
        }
    }
}
