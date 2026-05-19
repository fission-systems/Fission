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
        Commands::SourceSemanticCheck(args) | Commands::Check(args) => {
            lanes::source_semantic_check::run(args)
        }
        Commands::NirCheck(args) => lanes::nir_check::run(args),
    }
}
