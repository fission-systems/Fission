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
    if let Err(error) = fission_cli::cli::oneshot::run_oneshot() {
        eprintln!("{:?}", miette::Report::msg(format!("{error:#}")));
        std::process::exit(1);
    }
}
