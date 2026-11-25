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

fn exit(code: i32) -> ! {
    #[cfg(feature = "sentry")]
    fission_cli::telemetry::flush();
    std::process::exit(code)
}

fn main() {
    #[cfg(feature = "sentry")]
    let _telemetry = fission_cli::telemetry::init();
    // `--contract v1` runs this binary again under supervision and wraps the
    // result; without it nothing below changes.
    if let Some(code) = fission_cli::cli::contract::maybe_supervise() {
        exit(code);
    }
    if let Err(error) = fission_cli::cli::oneshot::run_oneshot() {
        if fission_cli::cli::contract::is_child() {
            fission_cli::cli::contract::emit_child_error(&error);
        } else {
            eprintln!("{:?}", miette::Report::msg(format!("{error:#}")));
        }
        exit(1);
    }
}
