//! Fission TUI — interactive AI chat interface.
//!
//! This module provides `run_tui`, the public entry point called by
//! `fission_cli ai chat`.

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use fission_ai::pipeline::AiPipeline;
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    Terminal, TerminalOptions, Viewport,
};
use tokio::sync::mpsc;
use std::io;

use crate::app::App;
use crate::events::{AppAction, poll_event};
use crate::ui;

/// Launch the interactive TUI chat session.
///
/// Uses ANSI 16-color palette (ui/mod.rs) to avoid the iTerm2 GPU Font Atlas
/// Cache Corruption bug. Color::Rgb creates unique atlas entries per (glyph,
/// color) pair, exhausting the cache. Named ANSI colors share atlas entries.
pub fn run_tui(mut pipeline: AiPipeline) -> Result<()> {
    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode().map_err(|e| anyhow::anyhow!("enable raw mode: {e}"))?;
    let backend = CrosstermBackend::new(io::stdout());

    // Detect terminal height dynamically, like Codex. Fall back to 24 if size
    // query fails (e.g. piped stdin). We leave 1 line of scrollback so the
    // shell prompt is visible above the TUI after exit.
    let term_height = crossterm::terminal::size()
        .map(|(_, h)| h.saturating_sub(1).max(8))
        .unwrap_or(24);

    // Use Viewport::Inline(height) — draws directly to the primary buffer,
    // NOT the alternate screen. VSCode aggressively destroys alternate-screen
    // buffers on tab-reparenting; inline mode is immune to that.
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions { viewport: Viewport::Inline(term_height) },
    )
    .map_err(|e| anyhow::anyhow!("create terminal: {e}"))?;

    terminal.clear().map_err(|e| anyhow::anyhow!("clear terminal: {e}"))?;

    let result = run_event_loop(&mut terminal, &mut pipeline);

    // ── Terminal restore ──────────────────────────────────────────────────────
    disable_raw_mode().ok();
    terminal.show_cursor().ok();

    result
}

// ── Stream delta messages ─────────────────────────────────────────────────────

enum TuiMsg {
    Delta(String),
    Done(String),    // full response text for history
    Error(String),
    /// Fired when background context collection finishes.
    ContextReady,
    /// Fired when available models are fetched from the API.
    ModelsFetched(Vec<String>),
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pipeline: &mut AiPipeline,
) -> Result<()> {
    let status_label = pipeline.status_label();
    let mut app = App::new(status_label);

    // Dedicated tokio runtime for driving async streaming tasks.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("streaming runtime: {e}"))?;

    // Unbounded channel: streaming task → main event loop.
    let (tx, mut rx) = mpsc::unbounded_channel::<TuiMsg>();

    // ── Bootstrap binary context snapshot ────────────────────────────────────
    // If a binary path is already set on the pipeline session (passed via CLI),
    // kick off background collection immediately so the AI has full context
    // from the very first message.
    let initial_binary_path = {
        let session = pipeline.session.lock().unwrap();
        session.binary_path.clone()
    };
    if let Some(bin_path) = initial_binary_path {
        app.context_loading = true;
        let pipeline_clone = pipeline.clone();
        let tx_ctx = tx.clone();
        rt.spawn(async move {
            pipeline_clone.init_binary_context(bin_path).await;
            let _ = tx_ctx.send(TuiMsg::ContextReady);
        });
    }

    loop {
        // ── Drain incoming stream deltas ──────────────────────────────────────
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TuiMsg::Delta(delta) => {
                    app.append_stream_delta(&delta);
                }
                TuiMsg::Done(full) => {
                    app.finish_assistant_stream();
                    pipeline.record_assistant_response(full);
                }
                TuiMsg::Error(e) => {
                    app.finish_assistant_stream();
                    app.append_stream_delta(&format!("\n⚠ {e}"));
                    app.finish_assistant_stream();
                }
                TuiMsg::ContextReady => {
                    app.context_loading = false;
                    // Update status label to reflect context is ready.
                    app.status_label = pipeline.status_label();
                }
                TuiMsg::ModelsFetched(models) => {
                    app.is_fetching_models = false;
                    if !models.is_empty() {
                        app.model_options = models;
                    } else {
                        // Fallback hardcoded if empty
                        app.model_options = vec!["gpt-4o".into(), "claude-3.5-sonnet".into(), "llama3".into()];
                    }
                    app.selected_model_idx = 0;
                }
            }
        }

        // (Source view synchronization removed for clean linear layout)

        // ── Render ────────────────────────────────────────────────────────────
        terminal.draw(|frame| ui::render(frame, &app))?;

        // ── Event poll (50ms timeout) ─────────────────────────────────────────
        let action = match poll_event().map_err(|e| anyhow::anyhow!("poll event: {e}"))? {
            Some(a) => a,
            None => continue,
        };

        match action {
            AppAction::Quit => break,
            AppAction::ToggleMode => {
                app.toggle_mode();
                pipeline.set_agent_mode(app.agent_mode);
            }
            AppAction::ToggleHelp => app.show_help = !app.show_help,
            AppAction::ToggleProviderMenu => {
                app.show_model_menu = false;
                app.toggle_provider_menu();
            },
            AppAction::ToggleModelMenu => {
                app.show_provider_menu = false;
                app.toggle_model_menu();
                if app.show_model_menu {
                    let tx2 = tx.clone();
                    let pipeline_clone = pipeline.clone();
                    rt.spawn(async move {
                        let models = pipeline_clone.fetch_models().await.unwrap_or_else(|_| vec![]);
                        let _ = tx2.send(TuiMsg::ModelsFetched(models));
                    });
                }
            },
            AppAction::Escape => {
                app.show_help = false;
                app.show_provider_menu = false;
                app.show_model_menu = false;
            }
            AppAction::InsertChar(c) if !app.streaming && !app.show_provider_menu && !app.show_model_menu => app.insert_char(c),
            AppAction::DeleteBack if !app.streaming && !app.show_provider_menu && !app.show_model_menu => app.delete_char_before_cursor(),
            AppAction::ScrollUp => {
                app.scroll_up();
            }
            AppAction::ScrollDown => {
                app.scroll_down();
            }
            AppAction::CursorUp => {
                if app.show_provider_menu {
                    app.provider_menu_up();
                } else if app.show_model_menu {
                    app.model_menu_up();
                } else if !app.streaming {
                    app.cursor_up();
                }
            }
            AppAction::CursorDown => {
                if app.show_provider_menu {
                    app.provider_menu_down();
                } else if app.show_model_menu {
                    app.model_menu_down();
                } else if !app.streaming {
                    app.cursor_down();
                }
            }
            AppAction::CycleProviderNext => {
                app.provider_menu_down();
                if let Some(kind) = app.get_selected_provider() {
                    match rt.block_on(pipeline.switch_provider(kind)) {
                        Ok(_) => { app.status_label = pipeline.status_label(); }
                        Err(_) => {} // Ignore silently on quick cycle
                    }
                }
            }
            AppAction::CycleProviderPrev => {
                app.provider_menu_up();
                if let Some(kind) = app.get_selected_provider() {
                    match rt.block_on(pipeline.switch_provider(kind)) {
                        Ok(_) => { app.status_label = pipeline.status_label(); }
                        Err(_) => {} // Ignore silently on quick cycle
                    }
                }
            }
            AppAction::CursorLeft if !app.streaming && !app.show_provider_menu && !app.show_model_menu => app.cursor_left(),
            AppAction::CursorRight if !app.streaming && !app.show_provider_menu && !app.show_model_menu => app.cursor_right(),
            AppAction::Submit if app.show_provider_menu => {
                if let Some(kind) = app.get_selected_provider() {
                    match rt.block_on(pipeline.switch_provider(kind)) {
                        Ok(_) => {
                            app.status_label = pipeline.status_label();
                        }
                        Err(e) => {
                            app.push_user("Switched Provider".to_string());
                            app.begin_assistant_stream();
                            app.append_stream_delta(&format!("\n⚠ Auth Error: {e}"));
                            app.finish_assistant_stream();
                            app.scroll_to_bottom();
                        }
                    }
                }
                app.show_provider_menu = false;
            }
            AppAction::Submit if app.show_model_menu => {
                if !app.is_fetching_models {
                    if let Some(model) = app.get_selected_model() {
                        match rt.block_on(pipeline.switch_model(model)) {
                            Ok(_) => {
                                app.status_label = pipeline.status_label();
                            }
                            Err(e) => {
                                app.push_user("Switched Model".to_string());
                                app.begin_assistant_stream();
                                app.append_stream_delta(&format!("\n⚠ Auth Error: {e}"));
                                app.finish_assistant_stream();
                                app.scroll_to_bottom();
                            }
                        }
                    }
                    app.show_model_menu = false;
                }
            }
            AppAction::Submit if !app.streaming && !app.input.trim().is_empty() && !app.show_provider_menu && !app.show_model_menu => {
                let user_text = app.take_input();
                app.push_user(user_text.clone());
                app.begin_assistant_stream();
                app.scroll_to_bottom();

                // Drive .send() synchronously on this thread (not Send-safe across threads),
                // then hand the resulting stream to the background rt.
                let stream_result = rt.block_on(pipeline.send(&user_text));

                match stream_result {
                    Ok(stream) => {
                        let tx2 = tx.clone();
                        rt.spawn(async move {
                            futures::pin_mut!(stream);
                            let mut full = String::new();
                            while let Some(chunk) = stream.next().await {
                                match chunk {
                                    Ok(c) => {
                                        if !c.delta.is_empty() {
                                            let _ = tx2.send(TuiMsg::Delta(c.delta.clone()));
                                            full.push_str(&c.delta);
                                        }
                                        if c.done { break; }
                                    }
                                    Err(e) => {
                                        let _ = tx2.send(TuiMsg::Error(e.to_string()));
                                        return;
                                    }
                                }
                            }
                            let _ = tx2.send(TuiMsg::Done(full));
                        });
                    }
                    Err(e) => {
                        app.finish_assistant_stream();
                        app.append_stream_delta(&format!("\n⚠ {e}"));
                        app.finish_assistant_stream();
                    }
                }
            }
            AppAction::Resize(_w, h) => {
                // When the terminal is resized, resize the inline viewport to the
                // new height and force a full repaint.
                let new_height = h.saturating_sub(1).max(8);
                let _ = terminal.resize(ratatui::layout::Rect {
                    x: 0, y: 0,
                    width: _w,
                    height: new_height,
                });
                let _ = terminal.clear();
            }
            _ => {}
        }
    }

    Ok(())
}
