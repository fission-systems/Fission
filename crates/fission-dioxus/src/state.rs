use dioxus::prelude::*;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::path::PathBuf;
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
}

// ── Function classification ──────────────────────────────────────────────────

/// Classification of the selected function — drives editor rendering decisions.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FunctionKind {
    /// Regular code function with a decompilable body.
    #[default]
    Code,
    /// Directly imported symbol — no executable body in this binary.
    Import { library: Option<String> },
    /// Import thunk / PLT stub — tiny JMP wrapper around an IAT entry.
    /// The decompiler output will appear self-recursive because the jump
    /// target carries the same IAT symbol name.
    Thunk { target: Option<u64> },
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
        Self { level: LogLevel::Info, message: msg.into() }
    }
    pub fn warn(msg: impl Into<String>) -> Self {
        Self { level: LogLevel::Warn, message: msg.into() }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self { level: LogLevel::Error, message: msg.into() }
    }
}

// ── AppState ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct AppState {
    /// Path of the currently loaded binary
    pub loaded_binary_path: Option<PathBuf>,
    /// Parsed binary (Arc for cheap O(1) clone across signal updates)
    pub binary: Option<Arc<LoadedBinary>>,
    /// Flat function list from the loaded binary (sorted by address)
    pub functions: Vec<FunctionInfo>,
    /// Currently selected function address
    pub current_function_addr: Option<u64>,
    /// Classification of the selected function (drives editor UX)
    pub current_function_kind: FunctionKind,
    /// Pseudocode output (NIR-faithful primary surface)
    pub decompiled_code: Option<String>,
    /// NIR surface (when available)
    pub decompiled_nir: Option<String>,
    /// Active editor tab
    pub active_tab: EditorTab,
    /// Active bottom panel tab
    pub active_bottom_tab: BottomTab,
    /// Log panel entries
    pub log_entries: Vec<LogEntry>,
    /// Binary loading in progress
    pub is_loading_binary: bool,
    /// Decompile in progress
    pub is_decompiling: bool,
    /// Search/filter string for the sidebar
    pub sidebar_search: String,
    /// Whether the command palette (Cmd+K) is open
    pub is_palette_open: bool,
    /// Current query string inside the palette
    pub palette_query: String,
    /// Keyboard-focused result index in the palette
    pub palette_focused: usize,
}

impl AppState {
    pub fn push_log(&mut self, entry: LogEntry) {
        // Keep at most 500 lines to avoid unbounded growth
        if self.log_entries.len() >= 500 {
            self.log_entries.remove(0);
        }
        self.log_entries.push(entry);
    }

    /// Functions filtered by the current sidebar search string.
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

    /// Active pseudocode string for the editor pane.
    pub fn editor_code(&self) -> Option<&str> {
        match self.active_tab {
            EditorTab::Pseudocode => self.decompiled_code.as_deref(),
            EditorTab::Nir => {
                self.decompiled_nir.as_deref().or(self.decompiled_code.as_deref())
            }
            EditorTab::Hex => None, // rendered separately
        }
    }

    /// Name of the currently selected function (for title display).
    pub fn current_function_name(&self) -> Option<String> {
        let addr = self.current_function_addr?;
        self.functions
            .iter()
            .find(|f| f.address == addr)
            .map(|f| f.name.clone())
    }

    /// Derive [`FunctionKind`] from raw `FunctionInfo` fields.
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

    /// Fuzzy-scored palette results, sorted descending by score.
    /// Returns at most `limit` entries.
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
}

/// Simple fuzzy match returning None if not all query chars are found.
/// Scores higher for: prefix match, consecutive chars, word-boundary hits.
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
            consecutive = if i > 0 && last + 1 == i { consecutive + 1 } else { 0 };
            score += 10 + consecutive as i32 * 6;
            if i == 0 { score += 20; }
            // Word-boundary bonus: char after `_` `:` `.` space
            if i > 0 && matches!(tchars[i - 1], '_' | ':' | '.' | ' ') {
                score += 12;
            }
            last = i;
            qi += 1;
        }
    }
    if qi == qchars.len() { Some(score) } else { None }
}

// ── Context helpers ─────────────────────────────────────────────────────────

pub fn use_app_state() -> Signal<AppState> {
    use_context::<Signal<AppState>>()
}

pub fn init_app_state() {
    let mut state = AppState::default();
    state.push_log(LogEntry::info("Fission UI initialized."));
    state.push_log(LogEntry::info("Open a binary via File → Open Binary…"));
    provide_context(Signal::new(state));
}
