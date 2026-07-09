#![allow(clippy::all)]

mod artifacts;
mod cli;
mod corpus;
mod diagnosis;
mod gates;
mod inventory;
mod lanes;
mod model;
mod report;

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    feature = "allocator-jemallocator",
    not(feature = "allocator-mimalloc")
))]
#[global_allocator]
static GLOBAL_ALLOCATOR: jemallocator::Jemalloc = jemallocator::Jemalloc;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};

fn main() {
    if let Err(error) = run_main() {
        let error = error.context(format!(
            "span trace:\n{}",
            fission_core::logging::capture_span_trace()
        ));
        eprintln!("{:?}", miette::Report::msg(format!("{error:#}")));
        std::process::exit(1);
    }
}

fn run_main() -> Result<()> {
    let cli = Cli::parse();
    lanes::nir_check::init_automation_logging(&lanes::nir_check::repo_root());
    match cli.command {
        Commands::NirCheck(args) => lanes::nir_check::run(args),
        Commands::SandboxCheck(args) => lanes::sandbox_check::run(lanes::sandbox_check::SandboxCheckArgs {
            release: args.release,
            no_build: args.no_build,
            fission_bin: args.fission_bin,
            output_dir: args.output_dir,
            fail_on_stop: args.fail_on_stop,
            dry_run: args.dry_run,
        }),
    }
}
