use crate::engine::{CallGraphNode, CfgGraphData, InstructionRow, XrefRow};
use dioxus::prelude::*;
use fission_analysis_protocol::StatusResponse;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::collections::HashMap;
use std::sync::Arc;

/// Rows mounted per batch in the sidebar function list -- see
/// `AppState::sidebar_render_limit`'s doc for why this exists.
pub const SIDEBAR_RENDER_BATCH: usize = 500;

// ── Tab types ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum EditorTab {
    #[default]
    Pseudocode,
    Nir,
    Disasm,
    Hex,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum BottomTab {
    #[default]
    Logs,
    Cfg,
    Xrefs,
}

/// A previously-computed decompile result, keyed by function address in
/// `AppState.decompile_cache`. Mirrors the fields `run_decompile` writes
/// into `AppState` on a fresh decompile.
#[derive(Clone, Debug, Default)]
pub struct CachedDecompile {
    pub code: String,
    pub code_nir: Option<String>,
    pub cfg: Option<CfgGraphData>,
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

// ── Sidebar kind filter ────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SidebarKindFilter {
    #[default]
    All,
    Code,
    Export,
    Import,
}

// ── Sidebar tab ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SidebarTab {
    #[default]
    Functions,
    Strings,
    CallGraph,
}

/// Sort key for the Call Graph tab's function table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CallGraphSort {
    #[default]
    CallersDesc,
    CalleesDesc,
    NameAsc,
}

// ── Binary string ────────────────────────────────────────────────────────────

/// A printable string extracted from the binary.
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryString {
    /// File offset of the string start.
    pub offset:  u64,
    /// The string value (UTF-8 decoded).
    pub value:   String,
    /// Section name if known (e.g. ".rdata", ".rodata").
    pub section: String,
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
    /// Unix timestamp in seconds (for display as "HH:MM:SS" in the log panel).
    /// Populated with `std::time::SystemTime::now()` on native; 0 on WASM.
    pub timestamp: u64,
}

impl LogEntry {
    fn now_ts() -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }
        #[cfg(target_arch = "wasm32")]
        { 0 }
    }

    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Info,
            message: msg.into(),
            timestamp: Self::now_ts(),
        }
    }
    pub fn warn(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Warn,
            message: msg.into(),
            timestamp: Self::now_ts(),
        }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Error,
            message: msg.into(),
            timestamp: Self::now_ts(),
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
    /// Last capability/resource report returned by the active backend.
    pub backend_status:    Option<StatusResponse>,
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
    /// Per-address decompile result cache: re-selecting a function already
    /// decompiled this session (e.g. clicking back to it after visiting
    /// others) restores instantly from here instead of re-running the whole
    /// decompile (network round-trip on web, full pipeline either way).
    /// Cleared whenever a new binary is loaded (see `open_binary`/Cmd+O).
    pub decompile_cache: HashMap<u64, CachedDecompile>,

    // ── Xrefs ────────────────────────────────────────────────────────────────
    pub current_xref_callers: Vec<XrefRow>,
    pub current_xref_callees: Vec<XrefRow>,
    pub is_loading_xrefs: bool,

    // ── Disassembly ─────────────────────────────────────────────────────────
    pub current_disasm: Vec<InstructionRow>,
    pub is_loading_disasm: bool,

    // ── Call graph (sidebar tab) ─────────────────────────────────────────────
    /// Whole-binary caller/callee counts, fetched once per binary (not
    /// per-function like xrefs) and cached here for the session -- see
    /// `reset_for_new_binary` for why this needs clearing on a new load.
    pub call_graph_nodes: Vec<CallGraphNode>,
    pub is_loading_callgraph: bool,
    pub call_graph_loaded: bool,
    pub call_graph_search: String,
    pub call_graph_sort: CallGraphSort,

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
    /// WASM only: `true` while the server is still running background CFG
    /// function discovery after an upload (see `engine::poll_functions`).
    /// The binary is already usable (loader symbols are in) while this is
    /// set; it just means more functions may still appear.
    pub is_analyzing: bool,

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

    // ── Sidebar ──────────────────────────────────────────────────────────────
    pub sidebar_search:    String,
    pub sidebar_tab:       SidebarTab,
    pub sidebar_kind_filter: SidebarKindFilter,
    /// How many rows of the current filtered function list are actually
    /// mounted in the DOM. `content-visibility: auto` only ever addressed
    /// paint/layout cost for off-screen rows -- with real binaries running
    /// into the tens of thousands of functions, constructing/diffing that
    /// many `FunctionListItem` VNodes on every render is itself the lag.
    /// Grows as the user scrolls near the bottom of the list (see
    /// `sidebar.rs`); reset to the initial batch whenever the filtered set
    /// changes (search/kind-filter edits).
    pub sidebar_render_limit: usize,

    // ── Strings browser ──────────────────────────────────────────────────────
    pub strings:           Vec<BinaryString>,
    pub strings_search:    String,
    /// Set when the user clicks a string in the sidebar: the Hex tab shows a
    /// window centred on this VA instead of the current function's bytes.
    /// Cleared by real navigation (`navigate_to`/`nav_back`/`nav_forward`) so
    /// it never lingers after the user moves to a different function.
    pub hex_view_target:   Option<u64>,

    // ── Rename map ───────────────────────────────────────────────────────────
    /// User-defined function name overrides: addr → custom name.
    pub rename_map:        HashMap<u64, String>,

    // ── Comments ─────────────────────────────────────────────────────────────
    /// User-authored per-address annotations, editable inline in the
    /// Disassembly view: instruction addr → comment text.
    pub comments:           HashMap<u64, String>,

    // ── Find bar (editor Ctrl+F) ─────────────────────────────────────────────
    pub find_query:        String,
    pub find_bar_open:     bool,
    /// 0-based index of the "current" match, for Enter/next/prev navigation.
    /// Clamped against the live match count on every render (see `Editor`).
    pub find_current_index: usize,

    // ── Command palette ─────────────────────────────────────────────────────
    pub is_palette_open: bool,
    pub palette_query: String,
    pub palette_focused: usize,

    // ── Decompile cache (native only) ────────────────────────────────────────
    /// Pre-built FactStore for the current binary. Populated on load, reused
    /// per decompile call to avoid rebuilding on every function click.
    #[cfg(not(target_arch = "wasm32"))]
    pub cached_facts: Option<Arc<crate::engine::FactStore>>,
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
            backend_status:    None,
            server_session_id: None,
            current_function_addr: None,
            current_function_kind: FunctionKind::Code,
            decompiled_code: None,
            decompiled_nir: None,
            current_cfg: None,
            decompile_cache: HashMap::new(),
            current_xref_callers: Vec::new(),
            current_xref_callees: Vec::new(),
            is_loading_xrefs: false,
            current_disasm: Vec::new(),
            is_loading_disasm: false,
            call_graph_nodes: Vec::new(),
            is_loading_callgraph: false,
            call_graph_loaded: false,
            call_graph_search: String::new(),
            call_graph_sort: CallGraphSort::default(),
            active_tab: EditorTab::Pseudocode,
            active_bottom_tab: BottomTab::Logs,
            log_entries: Vec::new(),
            is_loading_binary: false,
            is_decompiling: false,
            is_analyzing: false,
            is_batch_running: false,
            batch_done: 0,
            batch_total: 0,
            batch_cancel: false,
            nav_history: Vec::new(),
            nav_cursor: 0,
            sidebar_scroll_target: None,
            sidebar_search: String::new(),
            sidebar_tab:    SidebarTab::Functions,
            sidebar_kind_filter: SidebarKindFilter::All,
            sidebar_render_limit: SIDEBAR_RENDER_BATCH,
            strings:        Vec::new(),
            strings_search: String::new(),
            hex_view_target: None,
            rename_map:     HashMap::new(),
            comments:       HashMap::new(),
            find_query:     String::new(),
            find_bar_open:  false,
            find_current_index: 0,
            is_palette_open: false,
            palette_query: String::new(),
            palette_focused: 0,
            #[cfg(not(target_arch = "wasm32"))]
            cached_facts: None,
        }
    }

    /// True once a binary is loaded, regardless of backend architecture.
    /// `binary` only carries the actual `LoadedBinary` on the native path --
    /// the cloud/web path (`fission-serve`) keeps the binary server-side and
    /// leaves this `None` by design (see `engine::run_load`'s wasm arm), so
    /// checking `binary.is_some()` alone left every cloud/web session
    /// permanently "no binary loaded" in the UI despite a real, populated
    /// `functions` list. `binary_name` is set on both paths.
    pub fn has_binary_loaded(&self) -> bool {
        self.binary.is_some() || self.binary_name.is_some()
    }

    /// Reset every field scoped to the previously-loaded binary, before a
    /// caller applies a freshly loaded binary's own data (`functions`,
    /// `strings`, a sidecar's `rename_map`/`comments`, etc.).
    ///
    /// Each of these held an address, index, or cached view that's only
    /// meaningful for whichever binary was loaded when it was set. Left
    /// stale after switching binaries, several of them don't just look
    /// wrong -- they're actively misleading: `nav_history` full of the old
    /// binary's addresses means Back/Forward can jump to a location that
    /// means something completely different (or nothing) in the new one;
    /// a leftover `current_cfg`/`current_disasm`/xrefs view could still be
    /// showing before the user has picked any function in the new binary.
    /// Previously each of the four binary-load call sites (this crate's
    /// native paths, plus fission-web's own drop-to-load handler) built
    /// this reset list by hand and each one ended up incomplete in a
    /// different way -- e.g. `decompile_cache` was missing from all of
    /// them until it was the one that visibly broke; centralizing it here
    /// means there's exactly one list to keep complete.
    pub fn reset_for_new_binary(&mut self) {
        self.current_function_addr = None;
        self.current_function_kind = FunctionKind::Code;
        self.decompiled_code = None;
        self.decompiled_nir = None;
        self.current_cfg = None;
        self.decompile_cache.clear();
        self.current_xref_callers.clear();
        self.current_xref_callees.clear();
        self.is_loading_xrefs = false;
        self.current_disasm.clear();
        self.is_loading_disasm = false;
        self.call_graph_nodes.clear();
        self.is_loading_callgraph = false;
        self.call_graph_loaded = false;
        self.call_graph_search.clear();
        self.nav_history.clear();
        self.nav_cursor = 0;
        self.sidebar_scroll_target = None;
        self.sidebar_search.clear();
        self.strings_search.clear();
        self.hex_view_target = None;
        self.rename_map.clear();
        self.comments.clear();
        self.find_query.clear();
        self.find_bar_open = false;
        self.find_current_index = 0;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.cached_facts = None;
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
            EditorTab::Disasm => None,
            EditorTab::Hex => None,
        }
    }

    /// Returns the display name for the current function,
    /// honouring rename_map if set.
    pub fn current_function_name(&self) -> Option<String> {
        let addr = self.current_function_addr?;
        // User rename takes priority
        if let Some(renamed) = self.rename_map.get(&addr) {
            return Some(renamed.clone());
        }
        self.functions
            .iter()
            .find(|f| f.address == addr)
            .map(|f| f.name.clone())
    }

    /// Returns the *original* (FID/loader) name regardless of rename_map.
    pub fn original_function_name(&self, addr: u64) -> Option<String> {
        self.functions.iter().find(|f| f.address == addr).map(|f| f.name.clone())
    }

    /// Returns the display name for a given address (rename_map then loader).
    pub fn display_name(&self, addr: u64) -> String {
        self.rename_map.get(&addr).cloned()
            .or_else(|| self.functions.iter().find(|f| f.address == addr).map(|f| f.name.clone()))
            .unwrap_or_else(|| format!("sub_{addr:x}"))
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
        self.hex_view_target = None;
    }

    /// Navigate backward; returns the target address if available.
    pub fn nav_back(&mut self) -> Option<u64> {
        if self.nav_cursor == 0 {
            return None;
        }
        self.nav_cursor -= 1;
        let addr = self.nav_history[self.nav_cursor];
        self.sidebar_scroll_target = Some(addr);
        self.hex_view_target = None;
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
        self.hex_view_target = None;
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
