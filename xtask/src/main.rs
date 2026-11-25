//! Developer tasks for the Fission workspace.
//!
//! `cargo xtask preflight` runs, on this machine, the checks the CI Fast Gate
//! would run for the current branch, with the commands CI runs. Every check
//! either calls the same script the workflow step calls or reads its value
//! from the workflow file, so a change to CI changes preflight with it; the
//! tests in `preflight` fail when the two stop matching.

mod preflight;
mod workflow;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask preflight [--fast] [--full] [--debug | --release] [--base REF]

Runs the CI Fast Gate checks for the commits on this branch, with CI's own
commands (scripts/ci/*.sh, scripts/check/*.sh and the workflow files).

  --fast       skip nextest, doctests and optional-feature suites
  --full       check the whole workspace, as a workflow_dispatch run does
               (clippy uses the --workspace command main pushes run)
  --debug      test in the debug profile, where overflow checks fire (default)
  --release    test in the release profile
  --base REF   compare against REF instead of origin/main (no fetch is done)

Exit status is non-zero when any check fails; every selected check runs.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("preflight") => match preflight::Options::parse(&args[1..]) {
            Ok(options) => preflight::run(&options),
            Err(message) => {
                eprintln!("error: {message}\n\n{USAGE}");
                ExitCode::from(2)
            }
        },
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
