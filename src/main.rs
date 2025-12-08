//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! Entry point that handles CLI argument parsing and mode switching
//! between headless CLI and full GUI modes.

#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

mod app;
mod analysis;
mod debug;
mod script;
mod ui;

use clap::Parser;

/// Fission: Hybrid Dynamic Analysis Platform
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target binary path to analyze
    #[arg(short, long)]
    target: Option<String>,

    /// Run in headless mode (CLI only, no GUI)
    #[arg(long, default_value_t = false)]
    headless: bool,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> anyhow::Result<()> {
    // 1. Initialize logger with verbosity level
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        match std::env::args().filter(|a| a == "-v").count() {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        },
    ))
    .init();

    // 2. Parse command line arguments
    let args = Args::parse();

    log::info!("Fission Core Initialized");
    log::debug!("Target: {:?}", args.target);
    log::debug!("Headless: {}", args.headless);

    // 3. Branch based on execution mode
    if args.headless {
        // CLI mode: Run REPL in main thread
        println!("[*] Fission v{} - Headless Mode", env!("CARGO_PKG_VERSION"));
        ui::cli::run_cli()?;
    } else {
        // GUI mode: Run GUI in main thread
        println!("[*] Fission v{} - GUI Mode", env!("CARGO_PKG_VERSION"));

        // Run GUI main loop (wgpu/eframe)
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 720.0])
                .with_min_inner_size([800.0, 600.0])
                .with_title("Fission - Hybrid Analysis Platform"),
            ..Default::default()
        };

        eframe::run_native(
            "Fission",
            native_options,
            Box::new(|cc| {
                // Enable dark mode by default
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
                Box::new(ui::gui::FissionApp::default())
            }),
        )
        .map_err(|e| anyhow::anyhow!("GUI Error: {}", e))?;
    }

    Ok(())
}
