//! App state machine for the Fission TUI.

/// A rendered chat bubble (user or assistant).
#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role_label: String,
    pub content: String,
    pub is_streaming: bool,
}

#[derive(Clone)]
pub struct ProviderOption {
    pub kind: fission_ai::provider::ProviderKind,
    pub title: &'static str,
    pub description: &'static str,
}

/// Top-level application state.
pub struct App {
    /// Chat history entries for display.
    pub entries: Vec<ChatEntry>,
    /// Current user input buffer.
    pub input: String,
    /// Cursor position within `input` (byte offset).
    pub input_cursor: usize,
    /// Provider + model label for the status bar.
    pub status_label: String,
    /// Whether the app should quit on next tick.
    pub should_quit: bool,
    /// Whether a streaming response is in progress.
    pub streaming: bool,
    /// Vertical scroll offset for the chat viewport.
    pub scroll: u16,
    /// Whether to show the help overlay.
    pub show_help: bool,
    
    // ── Provider Menu ────────────────────────────────────────────────────────
    pub show_provider_menu: bool,
    pub provider_options: Vec<ProviderOption>,
    pub selected_provider_idx: usize,

    // ── Model Menu ───────────────────────────────────────────────────────────
    pub show_model_menu: bool,
    pub model_options: Vec<String>,
    pub selected_model_idx: usize,
    pub is_fetching_models: bool,

    // ── Agent Mode ───────────────────────────────────────────────────────────
    pub agent_mode: fission_ai::session::AgentMode,

    // ── Context State ────────────────────────────────────────────────────────
    /// Whether the binary context snapshot has been collected and injected.
    pub context_ready: bool,
    /// Whether context collection is currently in progress.
    pub context_loading: bool,
}

impl App {
    pub fn new(status_label: String) -> Self {
        Self {
            entries: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            status_label,
            should_quit: false,
            streaming: false,
            scroll: 0,
            show_help: false,
            show_provider_menu: false,
            provider_options: Vec::new(),
            selected_provider_idx: 0,
            show_model_menu: false,
            model_options: Vec::new(),
            selected_model_idx: 0,
            is_fetching_models: false,
            agent_mode: fission_ai::session::AgentMode::default(),
            context_ready: false,
            context_loading: false,
        }
    }

    pub fn toggle_mode(&mut self) {
        use fission_ai::session::AgentMode;
        self.agent_mode = match self.agent_mode {
            AgentMode::Analyst => AgentMode::Editor,
            AgentMode::Editor => AgentMode::Analyst,
        };
    }

    /// Push a user message entry (displayed immediately).
    pub fn push_user(&mut self, text: String) {
        self.entries.push(ChatEntry {
            role_label: "You".to_string(),
            content: text,
            is_streaming: false,
        });
    }

    /// Start a new streaming assistant entry.
    pub fn begin_assistant_stream(&mut self) {
        self.streaming = true;
        self.entries.push(ChatEntry {
            role_label: "Fission AI".to_string(),
            content: String::new(),
            is_streaming: true,
        });
    }

    /// Append a delta to the last (streaming) assistant entry.
    pub fn append_stream_delta(&mut self, delta: &str) {
        if let Some(last) = self.entries.last_mut() {
            if last.is_streaming {
                last.content.push_str(delta);
                // Always track the bottom while streaming.
                self.scroll = u16::MAX;
            }
        }
    }

    /// Finalise the streaming entry.
    pub fn finish_assistant_stream(&mut self) {
        self.streaming = false;
        if let Some(last) = self.entries.last_mut() {
            last.is_streaming = false;
        }
    }

    // ── Input management ──────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    pub fn delete_char_before_cursor(&mut self) {
        if self.input_cursor > 0 {
            let prev = self.input[..self.input_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.input_cursor = prev;
        }
    }

    pub fn cursor_left(&mut self) {
        if self.input_cursor > 0 {
            // Find the start of the previous char
            let prev = self.input[..self.input_cursor]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            self.input_cursor = prev;
        }
    }

    pub fn cursor_right(&mut self) {
        if self.input_cursor < self.input.len() {
            // Find the start of the next char
            if let Some(ch) = self.input[self.input_cursor..].chars().next() {
                self.input_cursor += ch.len_utf8();
            }
        }
    }

    pub fn cursor_up(&mut self) {
        let text_before = &self.input[..self.input_cursor];
        let current_line_idx = text_before.matches('\n').count();
        
        if current_line_idx == 0 {
            self.scroll_up();
            return;
        }
        
        let line_start_byte = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let current_col = self.input[line_start_byte..self.input_cursor].chars().count();
        
        let text_before_prev = &self.input[..line_start_byte.saturating_sub(1)];
        let prev_line_start = text_before_prev.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prev_line_text = &self.input[prev_line_start..line_start_byte.saturating_sub(1)];
        
        let mut target_byte = prev_line_start;
        for (i, c) in prev_line_text.char_indices().take(current_col) {
            target_byte = prev_line_start + i + c.len_utf8();
        }
        self.input_cursor = target_byte;
    }

    pub fn cursor_down(&mut self) {
        let text_before = &self.input[..self.input_cursor];
        let current_line_idx = text_before.matches('\n').count();
        let total_lines = self.input.matches('\n').count();
        
        if current_line_idx == total_lines {
            self.scroll_down();
            return;
        }
        
        let line_start_byte = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let current_col = self.input[line_start_byte..self.input_cursor].chars().count();
        
        let next_line_start = self.input[self.input_cursor..]
            .find('\n')
            .map(|i| self.input_cursor + i + 1)
            .unwrap_or(self.input.len());
            
        let next_line_end = self.input[next_line_start..]
            .find('\n')
            .map(|i| next_line_start + i)
            .unwrap_or(self.input.len());
            
        let next_line_text = &self.input[next_line_start..next_line_end];
        
        let mut target_byte = next_line_start;
        for (i, c) in next_line_text.char_indices().take(current_col) {
            target_byte = next_line_start + i + c.len_utf8();
        }
        self.input_cursor = target_byte;
    }

    pub fn take_input(&mut self) -> String {
        self.input_cursor = 0;
        std::mem::take(&mut self.input)
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(3);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = u16::MAX;
    }

    // ── Provider Menu ─────────────────────────────────────────────────────────

    pub fn toggle_provider_menu(&mut self) {
        self.show_provider_menu = !self.show_provider_menu;
        if self.show_provider_menu {
            self.provider_options = vec![
                ProviderOption {
                    kind: fission_ai::provider::ProviderKind::Codex,
                    title: "Codex",
                    description: "(ChatGPT OAuth - Recommended)",
                },
                ProviderOption {
                    kind: fission_ai::provider::ProviderKind::Copilot,
                    title: "GitHub Copilot",
                    description: "(GitHub Copilot Token)",
                },
                ProviderOption {
                    kind: fission_ai::provider::ProviderKind::OpenAi,
                    title: "OpenAI API",
                    description: "(OPENAI_API_KEY)",
                },
                ProviderOption {
                    kind: fission_ai::provider::ProviderKind::Ollama,
                    title: "Ollama",
                    description: "(Local via FISSION_AI_OLLAMA_URL)",
                },
            ];
            self.selected_provider_idx = 0;
        }
    }

    pub fn provider_menu_up(&mut self) {
        if self.provider_options.is_empty() { return; }
        if self.selected_provider_idx > 0 {
            self.selected_provider_idx -= 1;
        } else {
            self.selected_provider_idx = self.provider_options.len() - 1;
        }
    }

    pub fn provider_menu_down(&mut self) {
        if self.provider_options.is_empty() { return; }
        if self.selected_provider_idx + 1 < self.provider_options.len() {
            self.selected_provider_idx += 1;
        } else {
            self.selected_provider_idx = 0;
        }
    }

    pub fn get_selected_provider(&self) -> Option<fission_ai::provider::ProviderKind> {
        self.provider_options.get(self.selected_provider_idx).map(|p| p.kind.clone())
    }

    // ── Model Menu ────────────────────────────────────────────────────────────

    pub fn toggle_model_menu(&mut self) {
        self.show_model_menu = !self.show_model_menu;
        if self.show_model_menu {
            self.is_fetching_models = true;
            self.model_options.clear();
            self.selected_model_idx = 0;
        }
    }

    pub fn model_menu_up(&mut self) {
        if self.model_options.is_empty() { return; }
        if self.selected_model_idx > 0 {
            self.selected_model_idx -= 1;
        } else {
            self.selected_model_idx = self.model_options.len() - 1;
        }
    }

    pub fn model_menu_down(&mut self) {
        if self.model_options.is_empty() { return; }
        if self.selected_model_idx + 1 < self.model_options.len() {
            self.selected_model_idx += 1;
        } else {
            self.selected_model_idx = 0;
        }
    }

    pub fn get_selected_model(&self) -> Option<String> {
        self.model_options.get(self.selected_model_idx).cloned()
    }
}
