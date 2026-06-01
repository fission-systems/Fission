use serde::{Deserialize, Serialize};

/// Tracks the active reverse engineering focus state in the current session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReversingFocus {
    pub active_function_addr: Option<String>,
    pub active_function_name: Option<String>,
    pub last_disasm_range: Option<(u64, u64)>,
    pub verified_types: Vec<String>,
}

/// Zero-dependency context manager that dynamically budgets prompts,
/// truncates excessive tool outputs, and compacts message histories.
#[derive(Debug, Clone)]
pub struct ContextManager {
    pub max_char_budget: usize,
    pub max_tool_output_len: usize,
    pub focus: ReversingFocus,
}

impl ContextManager {
    /// Create a new ContextManager with character limit parameters.
    pub fn new(max_char_budget: usize, max_tool_output_len: usize) -> Self {
        Self {
            max_char_budget,
            max_tool_output_len,
            focus: ReversingFocus::default(),
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
            prompt.push_str(&format!("- **Last Disassembled Address Range**: {:#x} - {:#x}\n", start, end));
            has_focus = true;
        }
        if !self.focus.verified_types.is_empty() {
            prompt.push_str("- **Identified Structural Types**:\n");
            for t in &self.focus.verified_types {
                prompt.push_str(&format!("  - {}\n", t));
            }
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
        let total_len: usize = messages.iter()
            .map(|m| m.content.as_ref().map(|c| c.len()).unwrap_or(0))
            .sum();

        // If history is small enough, skip compaction
        if total_len <= self.max_char_budget || messages.len() <= 6 {
            return false;
        }

        let mut compacted = Vec::new();
        let mut has_system = false;

        // Keep original system prompt if it exists at index 0
        if let Some(first) = messages.first() {
            if first.role == crate::session::Role::System {
                compacted.push(first.clone());
                has_system = true;
            }
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
