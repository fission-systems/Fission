//! fission-tui standalone binary entry point.
//! Primarily used for development/testing; the main surface is `fission_cli ai chat`.

fn main() {
    eprintln!("Use `fission_cli ai chat` to launch the interactive AI chat TUI.");
    std::process::exit(1);
}
