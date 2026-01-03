//! TUI - Terminal User Interface for binary analysis
//!
//! A ratatui-based terminal interface for browsing and decompiling binaries.
//!
//! Usage:
//!   fission-tui <binary>

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};

#[cfg(feature = "native_decomp")]
use crate::analysis::decomp::ffi::DecompilerNative;

use crate::analysis::loader::{FunctionInfo, LoadedBinary};

use super::args::TuiArgs;

/// Entry point for TUI mode
pub fn run_tui() -> io::Result<()> {
    run()
}

/// Application state
struct App {
    /// Loaded binary
    binary: LoadedBinary,
    /// Binary data for decompiler
    binary_data: Vec<u8>,
    /// List of non-import functions
    functions: Vec<FunctionInfo>,
    /// Selected function index
    list_state: ListState,
    /// Decompiled code for selected function
    decompiled_code: String,
    /// Decompiler instance
    #[cfg(feature = "native_decomp")]
    decompiler: Option<DecompilerNative>,
    /// Status message
    status: String,
    /// Scroll position for code view
    scroll: u16,
    /// Should quit
    should_quit: bool,
}

impl App {
    fn new(binary: LoadedBinary, binary_data: Vec<u8>) -> Self {
        let functions: Vec<FunctionInfo> = binary
            .functions
            .iter()
            .filter(|f| !f.is_import)
            .cloned()
            .collect();

        let mut list_state = ListState::default();
        if !functions.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            binary,
            binary_data,
            functions,
            list_state,
            decompiled_code: "// Select a function and press Enter to decompile".to_string(),
            #[cfg(feature = "native_decomp")]
            decompiler: None,
            status: "Ready. ↑/↓:Navigate  Enter:Decompile  q:Quit".to_string(),
            scroll: 0,
            should_quit: false,
        }
    }

    fn selected_function(&self) -> Option<&FunctionInfo> {
        self.list_state
            .selected()
            .and_then(|i| self.functions.get(i))
    }

    fn select_next(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.functions.len() - 1 {
                self.list_state.select(Some(selected + 1));
            }
        }
    }

    fn select_previous(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected > 0 {
                self.list_state.select(Some(selected - 1));
            }
        }
    }

    fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(10);
    }

    fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(10);
    }

    #[cfg(feature = "native_decomp")]
    fn decompile_selected(&mut self) {
        let func = match self.selected_function() {
            Some(f) => f.clone(),
            None => return,
        };

        self.status = format!("Decompiling {} @ 0x{:x}...", func.name, func.address);

        // Initialize decompiler if needed
        if self.decompiler.is_none() {
            let sla_dir = std::env::current_dir()
                .unwrap()
                .join("ghidra_decompiler")
                .to_string_lossy()
                .into_owned();

            match DecompilerNative::new(&sla_dir) {
                Ok(mut decomp) => {
                    // Load binary
                    if let Err(e) = decomp.load_binary(
                        &self.binary_data,
                        self.binary.image_base,
                        self.binary.is_64bit,
                    ) {
                        self.decompiled_code = format!("// Error loading binary: {}", e);
                        self.status = "Error loading binary".to_string();
                        return;
                    }
                    decomp.add_symbols(&self.binary.iat_symbols);
                    self.decompiler = Some(decomp);
                }
                Err(e) => {
                    self.decompiled_code = format!("// Error creating decompiler: {}", e);
                    self.status = "Error creating decompiler".to_string();
                    return;
                }
            }
        }

        // Decompile
        if let Some(ref decomp) = self.decompiler {
            match decomp.decompile(func.address) {
                Ok(code) => {
                    self.decompiled_code = code;
                    self.scroll = 0;
                    self.status = format!(
                        "Decompiled {} ({} bytes)",
                        func.name,
                        self.decompiled_code.len()
                    );
                }
                Err(e) => {
                    self.decompiled_code = format!("// Error: {}", e);
                    self.status = format!("Error decompiling: {}", e);
                }
            }
        }
    }

    #[cfg(not(feature = "native_decomp"))]
    fn decompile_selected(&mut self) {
        self.decompiled_code =
            "// Decompilation requires native_decomp feature\n// Run with: cargo run --bin fission_tui --features \"tui,native_decomp\"".to_string();
        self.status = "native_decomp feature required".to_string();
    }
}

/// Main entry point (for bin/fission_tui.rs binary)
pub fn main() -> io::Result<()> {
    run_tui()
}

fn run() -> io::Result<()> {
    let cli = TuiArgs::parse();

    // Load binary
    let binary_data = std::fs::read(&cli.binary)?;
    let binary = LoadedBinary::from_bytes(
        binary_data.clone(),
        cli.binary.to_string_lossy().to_string(),
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let mut app = App::new(binary, binary_data);

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Main loop
    loop {
        terminal.draw(|frame| ui(frame, &mut app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                        KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                        KeyCode::Enter => app.decompile_selected(),
                        KeyCode::PageDown => app.page_down(),
                        KeyCode::PageUp => app.page_up(),
                        KeyCode::Char('d') => app.scroll_down(),
                        KeyCode::Char('u') => app.scroll_up(),
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Header
    let header = Paragraph::new(format!(
        " Fission TUI │ {} │ {} functions │ {} ({}-bit)",
        app.binary.path,
        app.functions.len(),
        app.binary.format,
        if app.binary.is_64bit { 64 } else { 32 }
    ))
    .block(Block::default().borders(Borders::ALL).title("Fission"))
    .style(Style::default().fg(Color::Cyan));
    frame.render_widget(header, chunks[0]);

    // Main content - split into function list and code view
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // Function list
    let items: Vec<ListItem> = app
        .functions
        .iter()
        .map(|f| ListItem::new(format!("0x{:x} {}", f.address, f.name)))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Functions"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, main_chunks[0], &mut app.list_state);

    // Code view
    let code_lines: Vec<&str> = app.decompiled_code.lines().collect();
    let visible_start = app.scroll as usize;
    let visible_lines: String = code_lines
        .iter()
        .skip(visible_start)
        .take(main_chunks[1].height as usize - 2)
        .map(|s| *s)
        .collect::<Vec<&str>>()
        .join("\n");

    let code_block = Paragraph::new(visible_lines)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Decompiled Code (line {}/{})",
            visible_start + 1,
            code_lines.len()
        )))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    frame.render_widget(code_block, main_chunks[1]);

    // Status bar
    let status = Paragraph::new(format!(" {}", app.status))
        .style(Style::default().bg(Color::Blue).fg(Color::White));
    frame.render_widget(status, chunks[2]);
}
