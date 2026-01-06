//! Fission TUI - Terminal UI binary
//!
//! Terminal user interface entry point

fn main() -> std::io::Result<()> {
    fission_cli::cli::tui::run_tui()
}
