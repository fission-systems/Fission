//! App state machine for the Fission TUI.

/// Which top-level view is currently displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Chat pane (default).
    #[default]
    Chat,
    /// Code explorer: top = disassembly, bottom = decompiled C.
    CodeExplorer,
}

/// Which panel inside Code Explorer has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    Disasm,
    Decomp,
}

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

/// State for the interactive `@` mention system.
#[derive(Debug, Clone)]
pub struct MentionState {
    pub query: String,
    pub options: Vec<String>,
    pub selected_idx: usize,
    pub start_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct SlashCommandState {
    pub start_cursor: usize,
    pub query: String,
    pub options: Vec<String>,
    pub selected_idx: usize,
}

#[derive(Debug, Clone)]
pub struct SessionHistoryState {
    pub options: Vec<(std::path::PathBuf, String)>,
    pub selected_idx: usize,
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
    /// Vertical scroll offset from the bottom for the chat viewport.
    pub offset_from_bottom: u16,
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

    // ── Mentions ─────────────────────────────────────────────────────────────
    pub mention_state: Option<MentionState>,
    
    // ── Slash Commands ───────────────────────────────────────────────────────
    pub slash_state: Option<SlashCommandState>,

    // ── Session History ──────────────────────────────────────────────────────
    pub session_history: Option<SessionHistoryState>,

    // ── Hybrid Code Explorer ─────────────────────────────────────────────────
    /// Current top-level view mode.
    pub view_mode: ViewMode,
    /// Which panel is focused in Code Explorer.
    pub active_panel: ActivePanel,
    /// Scroll offset (rows from top) for the Disassembly panel.
    pub disasm_scroll: u16,
    /// Scroll offset (rows from top) for the Decompiled-C panel.
    pub decomp_scroll: u16,
    /// Latest disassembly snippet cached by the pipeline.
    pub disasm_content: String,
    /// Latest decompiled-C content cached by the pipeline.
    pub decomp_content: String,
    /// Optional function name / address label for the explorer header.
    pub explorer_label: Option<String>,
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
            offset_from_bottom: 0,
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
            mention_state: None,
            slash_state: None,
            session_history: None,
            view_mode: ViewMode::Chat,
            active_panel: ActivePanel::Disasm,
            disasm_scroll: 0,
            decomp_scroll: 0,
            disasm_content: String::new(),
            decomp_content: String::new(),
            explorer_label: None,
        }
    }

    // ── View / Code Explorer helpers ──────────────────────────────────────────

    /// Toggle between Chat and Code Explorer views.
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Chat => ViewMode::CodeExplorer,
            ViewMode::CodeExplorer => ViewMode::Chat,
        };
    }

    /// Toggle focus between Disasm and Decomp panels.
    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Disasm => ActivePanel::Decomp,
            ActivePanel::Decomp => ActivePanel::Disasm,
        };
    }

    /// Scroll the focused panel up by `n` rows.
    pub fn explorer_scroll_up(&mut self, n: u16) {
        match self.active_panel {
            ActivePanel::Disasm => self.disasm_scroll = self.disasm_scroll.saturating_sub(n),
            ActivePanel::Decomp => self.decomp_scroll = self.decomp_scroll.saturating_sub(n),
        }
    }

    /// Scroll the focused panel down by `n` rows.
    pub fn explorer_scroll_down(&mut self, n: u16) {
        match self.active_panel {
            ActivePanel::Disasm => self.disasm_scroll = self.disasm_scroll.saturating_add(n),
            ActivePanel::Decomp => self.decomp_scroll = self.decomp_scroll.saturating_add(n),
        }
    }

    /// Update cached disasm/decomp content (called from pipeline interceptor via TuiMsg).
    pub fn update_explorer_content(
        &mut self,
        label: Option<String>,
        disasm: Option<String>,
        decomp: Option<String>,
    ) {
        if let Some(l) = label { self.explorer_label = Some(l); }
        if let Some(d) = disasm {
            self.disasm_content = d;
            self.disasm_scroll = 0;
        }
        if let Some(d) = decomp {
            self.decomp_content = d;
            self.decomp_scroll = 0;
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
                self.offset_from_bottom = 0;
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
        self.offset_from_bottom = self.offset_from_bottom.saturating_add(3);
    }

    pub fn scroll_down(&mut self) {
        self.offset_from_bottom = self.offset_from_bottom.saturating_sub(3);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.offset_from_bottom = 0;
    }

    // ── Mentions ──────────────────────────────────────────────────────────────

    pub fn start_mention(&mut self) {
        let options = get_workspace_files();
        self.mention_state = Some(MentionState {
            query: String::new(),
            options,
            selected_idx: 0,
            start_cursor: self.input_cursor, // byte offset where `@` is located
        });
    }

    pub fn cancel_mention(&mut self) {
        self.mention_state = None;
    }

    pub fn update_mention_query(&mut self) {
        if let Some(ref mut state) = self.mention_state {
            // The query is the text between start_cursor and current input_cursor
            if self.input_cursor >= state.start_cursor {
                state.query = self.input[state.start_cursor..self.input_cursor].to_string();
                
                // Refilter options (naive approach: just search for query as a substring)
                // In a real app we'd cache the full list, but doing it fast is fine.
                let all = get_workspace_files();
                state.options = all.into_iter()
                    .filter(|f| f.to_lowercase().contains(&state.query.to_lowercase()))
                    .take(20) // limit items
                    .collect();
                state.selected_idx = 0;
            } else {
                // We backspaced before the `@`
                self.mention_state = None;
            }
        }
    }

    pub fn mention_up(&mut self) {
        if let Some(ref mut state) = self.mention_state {
            if state.options.is_empty() { return; }
            if state.selected_idx > 0 {
                state.selected_idx -= 1;
            } else {
                state.selected_idx = state.options.len() - 1;
            }
        }
    }

    pub fn mention_down(&mut self) {
        if let Some(ref mut state) = self.mention_state {
            if state.options.is_empty() { return; }
            if state.selected_idx + 1 < state.options.len() {
                state.selected_idx += 1;
            } else {
                state.selected_idx = 0;
            }
        }
    }

    pub fn commit_mention(&mut self) {
        if let Some(state) = self.mention_state.take() {
            if let Some(selected) = state.options.get(state.selected_idx) {
                let prefix = self.input[..state.start_cursor.saturating_sub(1)].to_string();
                let suffix = self.input[self.input_cursor..].to_string();
                
                let insert_text = format!("@{} ", selected);
                self.input = format!("{}{}{}", prefix, insert_text, suffix);
                
                self.input_cursor = prefix.len() + insert_text.len();
            }
        }
    }

    // ── Slash Commands ───────────────────────────────────────────────────────

    pub fn start_slash_command(&mut self) {
        let options = vec![
            "clear".to_string(),
            "help".to_string(),
            "quit".to_string(),
            "history".to_string(),
            "provider".to_string(),
            "model".to_string(),
            "export".to_string(),
        ];
        self.slash_state = Some(SlashCommandState {
            start_cursor: self.input_cursor, // just after the `/`
            query: String::new(),
            options,
            selected_idx: 0,
        });
    }

    pub fn cancel_slash_command(&mut self) {
        self.slash_state = None;
    }

    pub fn update_slash_query(&mut self) {
        if let Some(state) = &mut self.slash_state {
            let query_str = &self.input[state.start_cursor..self.input_cursor];
            state.query = query_str.to_string();

            let all_commands = vec!["clear", "quit", "help", "provider", "model", "history"];
            state.options = all_commands
                .into_iter()
                .filter(|cmd| cmd.to_lowercase().contains(&state.query.to_lowercase()))
                .map(String::from)
                .collect();
            state.selected_idx = 0;
        }
    }

    pub fn slash_up(&mut self) {
        if let Some(state) = &mut self.slash_state {
            if state.selected_idx > 0 {
                state.selected_idx -= 1;
            } else if !state.options.is_empty() {
                state.selected_idx = state.options.len() - 1;
            }
        }
    }

    pub fn slash_down(&mut self) {
        if let Some(state) = &mut self.slash_state {
            if state.selected_idx + 1 < state.options.len() {
                state.selected_idx += 1;
            } else {
                state.selected_idx = 0;
            }
        }
    }

    pub fn commit_slash_command(&mut self) {
        if let Some(state) = self.slash_state.take() {
            if let Some(selected) = state.options.get(state.selected_idx) {
                let prefix = self.input[..state.start_cursor.saturating_sub(1)].to_string();
                let suffix = self.input[self.input_cursor..].to_string();
                
                let insert_text = format!("/{} ", selected);
                self.input = format!("{}{}{}", prefix, insert_text, suffix);
                
                self.input_cursor = prefix.len() + insert_text.len();
            }
        }
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

    // ── Session History ──────────────────────────────────────────────────────

    pub fn load_session_history(&mut self) {
        let mut options = Vec::new();
        if let Some(data_dir) = dirs::data_local_dir() {
            let session_dir = data_dir.join("fission").join("sessions");
            if session_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&session_dir) {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() && entry.path().extension().map(|s| s == "json").unwrap_or(false) {
                                let filename = entry.file_name().to_string_lossy().to_string();
                                let mut name = filename.replace(".json", "");
                                // Extract first few words if we saved them
                                if name.starts_with("session_") {
                                    name = name.replacen("session_", "", 1);
                                }
                                options.push((entry.path(), name));
                            }
                        }
                    }
                }
            }
        }
        
        // Sort descending by name (which starts with timestamp if we use UNIX time)
        options.sort_by(|a, b| b.1.cmp(&a.1));

        self.session_history = Some(SessionHistoryState {
            options,
            selected_idx: 0,
        });
    }

    pub fn close_session_history(&mut self) {
        self.session_history = None;
    }

    pub fn session_history_up(&mut self) {
        if let Some(state) = &mut self.session_history {
            if state.selected_idx > 0 {
                state.selected_idx -= 1;
            } else if !state.options.is_empty() {
                state.selected_idx = state.options.len() - 1;
            }
        }
    }

    pub fn session_history_down(&mut self) {
        if let Some(state) = &mut self.session_history {
            if state.selected_idx + 1 < state.options.len() {
                state.selected_idx += 1;
            } else {
                state.selected_idx = 0;
            }
        }
    }

    pub fn get_selected_session(&self) -> Option<std::path::PathBuf> {
        self.session_history.as_ref().and_then(|state| {
            state.options.get(state.selected_idx).map(|(p, _)| p.clone())
        })
    }

    pub fn save_current_session(&self, pipeline: &fission_ai::pipeline::AiPipeline) {
        if let Some(data_dir) = dirs::data_local_dir() {
            let session_dir = data_dir.join("fission").join("sessions");
            let _ = std::fs::create_dir_all(&session_dir);
            
            let messages = {
                let session = pipeline.session.lock().unwrap();
                if session.messages.is_empty() { return; }
                session.messages.clone()
            };

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            let path = session_dir.join(format!("session_{}.json", timestamp));
            if let Ok(json) = serde_json::to_string_pretty(&messages) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn export_to_markdown(&mut self) {
        if self.entries.is_empty() { return; }
        
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let filename = format!("fission_export_{}.md", timestamp);
        
        let mut md = String::new();
        md.push_str(&format!("# Fission AI Export - {}\n\n", timestamp));
        
        for entry in &self.entries {
            md.push_str(&format!("### {}\n\n", entry.role_label));
            md.push_str(&entry.content);
            md.push_str("\n\n---\n\n");
        }
        
        if std::fs::write(&filename, md).is_ok() {
            self.entries.push(ChatEntry {
                role_label: "System".to_string(),
                content: format!("Conversation successfully exported to `{}`.", filename),
                is_streaming: false,
            });
            self.scroll_to_bottom();
        }
    }
}

/// Recursively scans the current directory to populate the mention options.
fn get_workspace_files() -> Vec<String> {
    let mut results = Vec::new();
    let mut dirs_to_visit = vec![std::path::PathBuf::from(".")];
    let ignores = [".git", "target", "node_modules", "vendor", "artifacts"];

    while let Some(dir) = dirs_to_visit.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if ignores.contains(&file_name) {
                        continue;
                    }
                }
                
                if path.is_dir() {
                    dirs_to_visit.push(path);
                } else if path.is_file() {
                    if let Some(path_str) = path.to_str() {
                        let clean_path = path_str.strip_prefix("./").unwrap_or(path_str).to_string();
                        results.push(clean_path);
                    }
                }
            }
        }
    }
    
    // Sort for stable display
    results.sort();
    results
}
