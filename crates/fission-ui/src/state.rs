use crate::engine::{CfgGraphData, XrefRow};
use dioxus::prelude::*;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::sync::Arc;

// ── Tab types ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum EditorTab {
    #[default]
    Pseudocode,
    Nir,
    Hex,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum BottomTab {
    #[default]
    Logs,
    Cfg,
    Xrefs,
}

// ── Function classification ──────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum FunctionKind {
    #[default]
    Code,
    Import {
        library: Option<String>,
    },
    Thunk {
        target: Option<u64>,
    },
}

// ── Log entries ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

impl LogEntry {
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Info,
            message: msg.into(),
        }
    }
    pub fn warn(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Warn,
            message: msg.into(),
        }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Error,
            message: msg.into(),
        }
    }
}

// ── AppState ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    // ── Binary ──────────────────────────────────────────────────────────────
    pub binary_name: Option<String>,
    pub binary:      Option<Arc<LoadedBinary>>,
    pub functions:   Vec<FunctionInfo>,

    // ── Server (WASM mode) ───────────────────────────────────────────────────
    /// URL of the fission-serve instance (default: http://localhost:7331).
    /// Only used when running as WASM in the browser.
    pub server_url:        String,
    pub server_connected:  bool,
    /// Session token returned by POST /api/binary.
    /// All subsequent WASM API calls include this in the path.
    pub server_session_id: Option<String>,

    // ── Current selection ────────────────────────────────────────────────────
    pub current_function_addr: Option<u64>,
    pub current_function_kind: FunctionKind,

    // ── Decompile output ────────────────────────────────────────────────────
    pub decompiled_code: Option<String>,
    pub decompiled_nir: Option<String>,
    pub current_cfg: Option<CfgGraphData>,

    // ── Xrefs ────────────────────────────────────────────────────────────────
    pub current_xref_callers: Vec<XrefRow>,
    pub current_xref_callees: Vec<XrefRow>,
    pub is_loading_xrefs: bool,

    // ── Editor / panel state ─────────────────────────────────────────────────
    pub active_tab: EditorTab,
    pub active_bottom_tab: BottomTab,

    // ── Panel visibility / size ──────────────────────────────────────────────
    pub sidebar_visible: bool,
    pub bottom_panel_visible: bool,
    pub bottom_panel_height: f64, // px

    // ── Log ─────────────────────────────────────────────────────────────────
    pub log_entries: Vec<LogEntry>,

    // ── Async guards ────────────────────────────────────────────────────────
    pub is_loading_binary: bool,
    pub is_decompiling: bool,

    // ── Batch decompile ──────────────────────────────────────────────────────
    pub is_batch_running: bool,
    pub batch_done: usize,
    pub batch_total: usize,
    /// Set to true by the UI to request cancellation of the running batch.
    pub batch_cancel: bool,

    // ── Navigation history ────────────────────────────────────────────────────
    /// Addresses visited in order; new entries truncate forward history.
    pub nav_history: Vec<u64>,
    pub nav_cursor: usize, // index into nav_history pointing to current
    /// Set to Some(addr) to request the sidebar to scroll that item into view;
    /// cleared by the sidebar after it has processed it.
    pub sidebar_scroll_target: Option<u64>,

    // ── Sidebar search ───────────────────────────────────────────────────────
    pub sidebar_search: String,

    // ── Command palette ─────────────────────────────────────────────────────
    pub is_palette_open: bool,
    pub palette_query: String,
    pub palette_focused: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sidebar_visible: true,
            bottom_panel_visible: true,
            bottom_panel_height: 180.0,
            binary_name: None,
            binary: None,
            functions: Vec::new(),
            server_url:        "http://localhost:7331".to_string(),
            server_connected:  false,
            server_session_id: None,
            current_function_addr: None,
            current_function_kind: FunctionKind::Code,
            decompiled_code: None,
            decompiled_nir: None,
            current_cfg: None,
            current_xref_callers: Vec::new(),
            current_xref_callees: Vec::new(),
            is_loading_xrefs: false,
            active_tab: EditorTab::Pseudocode,
            active_bottom_tab: BottomTab::Logs,
            log_entries: Vec::new(),
            is_loading_binary: false,
            is_decompiling: false,
            is_batch_running: false,
            batch_done: 0,
            batch_total: 0,
            batch_cancel: false,
            nav_history: Vec::new(),
            nav_cursor: 0,
            sidebar_scroll_target: None,
            sidebar_search: String::new(),
            is_palette_open: false,
            palette_query: String::new(),
            palette_focused: 0,
        }
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.log_entries.len() >= 500 {
            self.log_entries.remove(0);
        }
        self.log_entries.push(entry);
    }

    pub fn filtered_functions(&self) -> Vec<&FunctionInfo> {
        let q = self.sidebar_search.to_lowercase();
        self.functions
            .iter()
            .filter(|f| {
                q.is_empty()
                    || f.name.to_lowercase().contains(&q)
                    || format!("{:x}", f.address).contains(&q)
            })
            .collect()
    }

    pub fn editor_code(&self) -> Option<&str> {
        match self.active_tab {
            EditorTab::Pseudocode => self.decompiled_code.as_deref(),
            EditorTab::Nir => self
                .decompiled_nir
                .as_deref()
                .or(self.decompiled_code.as_deref()),
            EditorTab::Hex => None,
        }
    }

    pub fn current_function_name(&self) -> Option<String> {
        let addr = self.current_function_addr?;
        self.functions
            .iter()
            .find(|f| f.address == addr)
            .map(|f| f.name.clone())
    }

    pub fn classify_function(info: &fission_loader::loader::FunctionInfo) -> FunctionKind {
        if info.is_import && !info.is_thunk_like {
            FunctionKind::Import {
                library: info.external_library.clone(),
            }
        } else if info.is_thunk_like {
            FunctionKind::Thunk {
                target: info.thunk_target,
            }
        } else {
            FunctionKind::Code
        }
    }

    pub fn palette_results(&self, limit: usize) -> Vec<(i32, &FunctionInfo)> {
        let q = self.palette_query.to_lowercase();
        let mut scored: Vec<(i32, &FunctionInfo)> = self
            .functions
            .iter()
            .filter_map(|f| {
                let score = fuzzy_score(&q, &f.name.to_lowercase());
                score.map(|s| (s, f))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(limit);
        scored
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    /// Toggle bottom panel visibility.
    pub fn toggle_bottom_panel(&mut self) {
        self.bottom_panel_visible = !self.bottom_panel_visible;
    }

    /// Push a new navigation address, truncating any forward history.
    /// No-op if addr is the same as the current entry.
    pub fn navigate_to(&mut self, addr: u64) {
        if self.nav_history.get(self.nav_cursor) == Some(&addr) {
            // Already at this address
            return;
        }
        // Truncate forward history
        if !self.nav_history.is_empty() {
            self.nav_history.truncate(self.nav_cursor + 1);
        }
        self.nav_history.push(addr);
        if self.nav_history.len() > 50 {
            self.nav_history.remove(0);
        }
        self.nav_cursor = self.nav_history.len().saturating_sub(1);
        self.sidebar_scroll_target = Some(addr);
    }

    /// Navigate backward; returns the target address if available.
    pub fn nav_back(&mut self) -> Option<u64> {
        if self.nav_cursor == 0 {
            return None;
        }
        self.nav_cursor -= 1;
        let addr = self.nav_history[self.nav_cursor];
        self.sidebar_scroll_target = Some(addr);
        Some(addr)
    }

    /// Navigate forward; returns the target address if available.
    pub fn nav_forward(&mut self) -> Option<u64> {
        if self.nav_cursor + 1 >= self.nav_history.len() {
            return None;
        }
        self.nav_cursor += 1;
        let addr = self.nav_history[self.nav_cursor];
        self.sidebar_scroll_target = Some(addr);
        Some(addr)
    }

    /// Whether backward navigation is possible.
    pub fn can_nav_back(&self) -> bool {
        self.nav_cursor > 0 && !self.nav_history.is_empty()
    }

    /// Whether forward navigation is possible.
    pub fn can_nav_forward(&self) -> bool {
        self.nav_cursor + 1 < self.nav_history.len()
    }
}

/// Simple fuzzy match returning None if not all query chars are found.
pub fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let qchars: Vec<char> = query.chars().collect();
    let tchars: Vec<char> = target.chars().collect();
    let mut qi = 0usize;
    let mut score = 0i32;
    let mut last = 0usize;
    let mut consecutive = 0usize;

    for (i, &tc) in tchars.iter().enumerate() {
        if qi < qchars.len() && tc == qchars[qi] {
            consecutive = if i > 0 && last + 1 == i {
                consecutive + 1
            } else {
                0
            };
            score += 10 + consecutive as i32 * 6;
            if i == 0 {
                score += 20;
            }
            if i > 0 && matches!(tchars[i - 1], '_' | ':' | '.' | ' ') {
                score += 12;
            }
            last = i;
            qi += 1;
        }
    }
    if qi == qchars.len() {
        Some(score)
    } else {
        None
    }
}

// ── Context helpers ─────────────────────────────────────────────────────────

pub fn use_app_state() -> Signal<AppState> {
    use_context::<Signal<AppState>>()
}

pub fn init_app_state() {
    let mut state = AppState::new();
    state.push_log(LogEntry::info("Fission UI initialized."));
    state.push_log(LogEntry::info(
        "Open a binary via File → Open Binary…  or drag and drop a file.",
    ));
    use_context_provider(|| Signal::new(state));
}
