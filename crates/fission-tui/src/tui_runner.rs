//! Fission TUI — interactive AI chat interface.
//!
//! This module provides `run_tui`, the public entry point called by
//! `fission_cli ai chat`.

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use fission_ai::pipeline::AiPipeline;
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;
use tokio::sync::mpsc;
use std::io;

use crate::app::App;
use crate::events::{AppAction, poll_event};
use crate::ui;

/// Launch the interactive TUI chat session.
///
/// Takes ownership of the terminal for the session duration and restores it
/// on exit or panic.
pub fn run_tui(mut pipeline: AiPipeline) -> Result<()> {
    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode().map_err(|e| anyhow::anyhow!("enable raw mode: {e}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| anyhow::anyhow!("enter alternate screen: {e}"))?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|e| anyhow::anyhow!("create terminal: {e}"))?;
    terminal.clear().map_err(|e| anyhow::anyhow!("clear terminal: {e}"))?;

    let result = run_event_loop(&mut terminal, &mut pipeline);

    // ── Terminal restore ──────────────────────────────────────────────────────
    disable_raw_mode().ok();
    execute!(io::stdout(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

// ── Stream delta messages ─────────────────────────────────────────────────────

enum TuiMsg {
    Delta(String),
    Done(String),    // full response text for history
    Error(String),
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pipeline: &mut AiPipeline,
) -> Result<()> {
    let status_label = pipeline.status_label();
    let mut app = App::new(status_label);

    // Unbounded channel: streaming task → main event loop.
    let (tx, mut rx) = mpsc::unbounded_channel::<TuiMsg>();

    // Dedicated tokio runtime for driving async streaming tasks.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("streaming runtime: {e}"))?;

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
                    app.append_stream_delta(&format!("\n⚠ Error: {e}"));
                    app.finish_assistant_stream();
                }
            }
        }

        // ── Render ────────────────────────────────────────────────────────────
        terminal.draw(|frame| ui::render(frame, &app))?;

        // ── Event poll (50ms timeout) ─────────────────────────────────────────
        let action = match poll_event().map_err(|e| anyhow::anyhow!("poll event: {e}"))? {
            Some(a) => a,
            None => continue,
        };

        match action {
            AppAction::Quit => break,
            AppAction::ToggleHelp => app.show_help = !app.show_help,
            AppAction::InsertChar(c) if !app.streaming => app.insert_char(c),
            AppAction::DeleteBack if !app.streaming => app.delete_char_before_cursor(),
            AppAction::ScrollUp => app.scroll_up(),
            AppAction::ScrollDown => app.scroll_down(),
            AppAction::Submit if !app.streaming && !app.input.trim().is_empty() => {
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
            _ => {}
        }
    }

    Ok(())
}
