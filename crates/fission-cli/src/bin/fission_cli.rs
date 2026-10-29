//! Fission CLI - One-shot binary
//!
//! Single-command execution mode entry point

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    feature = "allocator-jemallocator",
    not(feature = "allocator-mimalloc")
))]
#[global_allocator]
static GLOBAL_ALLOCATOR: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn main() {
    // `--contract v1` runs this binary again under supervision and wraps the
    // result; without it nothing below changes.
    if let Some(code) = fission_cli::cli::contract::maybe_supervise() {
        std::process::exit(code);
    }
    if let Err(error) = fission_cli::cli::oneshot::run_oneshot() {
        if fission_cli::cli::contract::is_child() {
            fission_cli::cli::contract::emit_child_error(&error);
        } else {
            eprintln!("{:?}", miette::Report::msg(format!("{error:#}")));
        }
        std::process::exit(1);
    }
}
