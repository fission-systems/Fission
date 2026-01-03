//! Fission TUI - Terminal UI binary
//!
//! Terminal user interface entry point

fn main() -> std::io::Result<()> {
    fission::cli::tui::run_tui()
}
