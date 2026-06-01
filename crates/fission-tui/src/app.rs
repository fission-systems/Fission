//! App state machine for the Fission TUI.

/// A rendered chat bubble (user or assistant).
#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role_label: String,
    pub content: String,
    pub is_streaming: bool,
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
        }
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
                // Auto-scroll to bottom while streaming.
                self.scroll = self.scroll.saturating_add(0); // will be recalculated by render
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
}
