//! Fission CLI - One-shot binary
//!
//! Single-command execution mode entry point

fn main() -> std::io::Result<()> {
    fission::cli::oneshot::run_oneshot()
}
