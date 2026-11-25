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
    // `--contract v1` runs this binary again under supervision and wraps the
    // result; without it nothing below changes.
    if let Some(code) = fission_cli::cli::contract::maybe_supervise() {
        std::process::exit(code);
    }
    // Error reporting starts only in the process that does the work: a plain
    // run, or the contract child (which inherits FISSION_SENTRY/SENTRY_DSN).
    // The supervisor never reports, so one defect is one event: a child
    // panic reaches the supervisor only as exit status 101 and stderr text,
    // which carries no location and is free text the policy never sends.
    #[cfg(feature = "sentry")]
    let _telemetry = fission_cli::telemetry::init();
    if let Err(error) = fission_cli::cli::oneshot::run_oneshot() {
        if fission_cli::cli::contract::is_child() {
            fission_cli::cli::contract::emit_child_error(&error);
        } else {
            eprintln!("{:?}", miette::Report::msg(format!("{error:#}")));
        }
        exit(1);
    }
}
