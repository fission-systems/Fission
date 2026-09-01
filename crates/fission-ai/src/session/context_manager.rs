use serde::{Deserialize, Serialize};

// ── Binary Snapshot ───────────────────────────────────────────────────────────

/// One-shot snapshot of static binary facts collected when binary_path is set.
/// Injected as a prefix in the system prompt so the AI has constant background knowledge.
#[derive(Debug, Clone, Default)]
pub struct BinarySnapshot {
    /// One-line summary: arch, entry, file type, etc.
    pub meta: String,
    /// Up to N function names/addresses (symbols).
    pub functions: Vec<String>,
    /// Up to N interesting strings from the binary.
    pub strings: Vec<String>,
}

impl BinarySnapshot {
    pub const MAX_FUNCTIONS: usize = 10;
    pub const MAX_STRINGS: usize = 20;

    /// Format this snapshot as a system-prompt section.
    pub fn format_prompt(&self) -> String {
        if self.meta.is_empty() && self.functions.is_empty() && self.strings.is_empty() {
            return String::new();
        }

        let mut out = String::from("\n### Binary Context Snapshot\n");

        if !self.meta.is_empty() {
            out.push_str(&format!("**Metadata:**\n```\n{}\n```\n\n", self.meta));
        }

        if !self.functions.is_empty() {
            out.push_str(&format!(
                "**Known Functions (top {}):**\n",
                self.functions.len()
            ));
            for f in &self.functions {
                out.push_str(&format!("- {}\n", f));
            }
            out.push('\n');
        }

        if !self.strings.is_empty() {
            out.push_str(&format!(
                "**Notable Strings (top {}):**\n",
                self.strings.len()
            ));
            for s in &self.strings {
                out.push_str(&format!("- `{}`\n", s));
            }
            out.push('\n');
        }

        out
    }
}

// ── Reversing Focus ───────────────────────────────────────────────────────────

/// Tracks the active reverse engineering focus state in the current session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReversingFocus {
    pub active_function_addr: Option<String>,
    pub active_function_name: Option<String>,
    pub last_disasm_range: Option<(u64, u64)>,
    pub verified_types: Vec<String>,

    /// Functions that call the currently focused function (callers).
    pub xrefs_callers: Vec<String>,
    /// Functions that the currently focused function calls (callees).
    pub xrefs_callees: Vec<String>,
    /// Most recent decompiled C pseudocode snippet for the focused function (truncated).
    pub decomp_snippet: Option<String>,
    /// Most recent disassembled assembly snippet for the focused function (truncated).
    pub disasm_snippet: Option<String>,
}

impl ReversingFocus {
    pub const MAX_DECOMP_SNIPPET_LEN: usize = 2000;
    pub const MAX_DISASM_SNIPPET_LEN: usize = 4000;

    /// Reset xrefs and decomp when the focus address changes.
    pub fn set_focus(&mut self, addr: String, name: Option<String>) {
        if self.active_function_addr.as_ref() != Some(&addr) {
            // Address changed: clear stale xrefs and decomp
            self.xrefs_callers.clear();
            self.xrefs_callees.clear();
            self.decomp_snippet = None;
            self.disasm_snippet = None;
        }
        self.active_function_addr = Some(addr);
        if name.is_some() {
            self.active_function_name = name;
        }
    }

    /// Store a decompiled snippet, truncating if necessary.
    ///
    /// By characters: a byte cut lands inside a multi-byte character and
    /// panics, and these snippets carry binary-derived text -- a Go symbol's
    /// `·` reaches disassembly listings through the operand annotations.
    pub fn set_decomp_snippet(&mut self, snippet: String) {
        self.decomp_snippet = Some(truncate_snippet(snippet, Self::MAX_DECOMP_SNIPPET_LEN));
    }

    /// Store a disassembled snippet, truncating if necessary.
    pub fn set_disasm_snippet(&mut self, snippet: String) {
        self.disasm_snippet = Some(truncate_snippet(snippet, Self::MAX_DISASM_SNIPPET_LEN));
    }
}

/// Cut a snippet to `max_chars` characters, marking that it was cut.
///
/// Characters rather than bytes: slicing a `&str` at a byte offset panics when
/// the offset lands inside a multi-byte character, and these snippets carry
/// binary-derived text.
fn truncate_snippet(snippet: String, max_chars: usize) -> String {
    if snippet.chars().count() <= max_chars {
        return snippet;
    }
    let kept: String = snippet.chars().take(max_chars).collect();
    format!("{kept}... [truncated]")
}

// ── Context Manager ───────────────────────────────────────────────────────────

/// Zero-dependency context manager that dynamically budgets prompts,
/// truncates excessive tool outputs, and compacts message histories.
#[derive(Debug, Clone)]
pub struct ContextManager {
    pub max_char_budget: usize,
    pub max_tool_output_len: usize,
    pub focus: ReversingFocus,
    /// Snapshot of binary-level static facts. Set once when binary_path is resolved.
    pub snapshot: Option<BinarySnapshot>,
}

impl ContextManager {
    /// Create a new ContextManager with character limit parameters.
    pub fn new(max_char_budget: usize, max_tool_output_len: usize) -> Self {
        Self {
            max_char_budget,
            max_tool_output_len,
            focus: ReversingFocus::default(),
            snapshot: None,
        }
    }

    /// Truncates a tool output if it exceeds the configured maximum length to prevent context window explosion.
    pub fn process_tool_output(&self, tool_name: &str, output: String) -> String {
        if output.len() > self.max_tool_output_len {
            format!(
                "{}\n\n... [Truncated {} characters of output from {} to prevent context exhaustion. Use a narrower address range or query parameters if needed] ...",
                &output[..self.max_tool_output_len],
                output.len() - self.max_tool_output_len,
                tool_name
            )
        } else {
            output
        }
    }

    /// Return the snapshot system-prompt prefix (empty string if no snapshot yet).
    pub fn format_binary_snapshot(&self) -> String {
        self.snapshot
            .as_ref()
            .map(|s| s.format_prompt())
            .unwrap_or_default()
    }

    /// Formats the active focus state into a structured Markdown prompt to keep the LLM grounded.
    pub fn format_focus_prompt(&self) -> String {
        let mut prompt = String::from("\n### Active Reversing Focus State\n");
        let mut has_focus = false;

        if let Some(addr) = &self.focus.active_function_addr {
            prompt.push_str(&format!("- **Current Function Address**: {}\n", addr));
            has_focus = true;
        }
        if let Some(name) = &self.focus.active_function_name {
            prompt.push_str(&format!("- **Current Function Name**: {}\n", name));
            has_focus = true;
        }
        if let Some((start, end)) = self.focus.last_disasm_range {
            prompt.push_str(&format!(
                "- **Last Disassembled Address Range**: {:#x} - {:#x}\n",
                start, end
            ));
            has_focus = true;
        }
        if !self.focus.verified_types.is_empty() {
            prompt.push_str("- **Identified Structural Types**:\n");
            for t in &self.focus.verified_types {
                prompt.push_str(&format!("  - {}\n", t));
            }
            has_focus = true;
        }
        if !self.focus.xrefs_callers.is_empty() {
            prompt.push_str("- **Called By (Callers)**:\n");
            for c in &self.focus.xrefs_callers {
                prompt.push_str(&format!("  - {}\n", c));
            }
            has_focus = true;
        }
        if !self.focus.xrefs_callees.is_empty() {
            prompt.push_str("- **Calls (Callees)**:\n");
            for c in &self.focus.xrefs_callees {
                prompt.push_str(&format!("  - {}\n", c));
            }
            has_focus = true;
        }
        if let Some(snippet) = &self.focus.decomp_snippet {
            prompt.push_str("- **Decompiled Pseudocode (current function)**:\n```c\n");
            prompt.push_str(snippet);
            prompt.push_str("\n```\n");
            has_focus = true;
        }

        if !has_focus {
            prompt.push_str("- No active target function or address range is set yet. Explore using disasm/xrefs tools.\n");
        }
        prompt
    }

    /// Checks the total character length of the message list, squashing older messages
    /// into a single high-level summary if they exceed the maximum character budget.
    pub fn compact_history(&self, messages: &mut Vec<crate::session::Message>) -> bool {
        let total_len: usize = messages
            .iter()
            .map(|m| m.content.as_ref().map(|c| c.len()).unwrap_or(0))
            .sum();

        // If history is small enough, skip compaction
        if total_len <= self.max_char_budget || messages.len() <= 6 {
            return false;
        }

        let mut compacted = Vec::new();
        let mut has_system = false;

        // Keep original system prompt if it exists at index 0
        if let Some(first) = messages.first()
            && first.role == crate::session::Role::System
        {
            compacted.push(first.clone());
            has_system = true;
        }

        // Add context compaction sentinel
        compacted.push(crate::session::Message::system(
            "[System: Previous conversation history compacted to free up context budget. High-level analysis mapping, disassembly, and function references have been processed.]"
        ));

        // Keep the last 4 messages (2 full dialogue turns) intact
        let end_idx = messages.len().saturating_sub(4);
        let start_idx = if has_system { 1 } else { 0 };

        if start_idx < end_idx {
            compacted.extend(messages[end_idx..].iter().cloned());
            *messages = compacted;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod snippet_truncation_tests {
    use super::{ReversingFocus, truncate_snippet};

    /// Disassembly listings carry symbol names, and a Go symbol's `·` is two
    /// bytes -- so a byte cut can land inside one. Reviewed rather than
    /// reproduced when it was fixed, so the behaviour past the limit is
    /// pinned here.
    #[test]
    fn a_snippet_past_the_limit_is_cut_between_characters() {
        // The real character: a Go symbol carries U+00B7, which is two
        // bytes. Offset by one ASCII byte so the byte at the limit falls
        // inside one rather than between two.
        let separator = char::from_u32(0xb7).expect("valid scalar");
        let listing = format!(
            "|{}",
            separator
                .to_string()
                .repeat(ReversingFocus::MAX_DISASM_SNIPPET_LEN + 100)
        );
        assert!(!listing.is_char_boundary(ReversingFocus::MAX_DISASM_SNIPPET_LEN));

        let cut = truncate_snippet(listing, ReversingFocus::MAX_DISASM_SNIPPET_LEN);
        assert!(cut.ends_with("... [truncated]"), "{cut}");
    }

    #[test]
    fn the_setters_accept_multi_byte_text() {
        let mut focus = ReversingFocus::default();
        let go_symbol = format!("{}main·main\n", "call ".repeat(1200));
        focus.set_disasm_snippet(go_symbol.clone());
        focus.set_decomp_snippet(go_symbol);
        assert!(focus.disasm_snippet.is_some());
        assert!(focus.decomp_snippet.is_some());
    }
}
