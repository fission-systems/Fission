//! Developer tasks for the Fission workspace.
//!
//! `cargo xtask preflight` runs, on this machine, the checks the CI Fast Gate
//! would run for the current branch, with the commands CI runs. Every check
//! either calls the same script the workflow step calls or reads its value
//! from the workflow file, so a change to CI changes preflight with it; the
//! tests in `preflight` fail when the two stop matching.
//!
//! `cargo xtask cleanup` reports, and with `--apply` removes, stale build
//! directories and finished worktrees of this repository.
//!
//! `cargo xtask remote` runs one cargo command on a hosted runner instead of
//! this machine (see `remote`).

mod cleanup;
mod preflight;
mod remote;
mod workflow;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask preflight [--fast] [--full] [--debug | --release] [--base REF]
       cargo xtask cleanup [--apply] [--include-main-target] [--min-age-hours N]
                           [--base REF] [--no-gh] [--hints DIR | --no-hints]
       cargo xtask remote [--os linux|macos|windows] [--artifact PATH]... [--keep]
                          [--] <cargo arguments...>

preflight:

Runs the CI Fast Gate checks for the commits on this branch, with CI's own
commands (scripts/ci/*.sh, scripts/check/*.sh and the workflow files).

  --fast       skip nextest, doctests and optional-feature suites
  --full       check the whole workspace, as a workflow_dispatch run does
               (clippy uses the --workspace command main pushes run)
  --debug      test in the debug profile, where overflow checks fire (default)
  --release    test in the release profile
  --base REF   compare against REF instead of origin/main (no fetch is done)

Exit status is non-zero when any check fails; every selected check runs.

cleanup:
Lists stale build output (`target`, `target-*` in the main checkout and in
every worktree, `.claude/worktrees/*` included) and worktrees whose branch is
merged or whose PR is merged/closed. Read-only unless --apply is given.

  --apply                remove the listed candidates (printed first)
  --include-main-target  also consider the main checkout's `target/`
  --min-age-hours N      keep anything written or checked out within N
                         hours (default 24)
  --base REF             merged means contained in REF (default origin/main;
                         no fetch is done)
  --no-gh                skip the `gh pr list` lookup (ancestry only)
  --hints DIR            size DIR's entries as hints, never removed
                         (default ~/fission-benchmark); --no-hints skips it

Never removed: the main checkout, a worktree that is dirty, has commits on
no remote ref, is locked, or is a process's cwd; a build dir a process uses.

remote:
Runs `cargo <arguments>` on a GitHub-hosted runner (.github/workflows/remote.yml)
instead of this machine, and prints its output and exit status. The working
tree, uncommitted and untracked files included, is pushed as a temporary
`remote/<sha>` branch, deleted afterwards; the branch, index and stash here are
untouched. Needs `gh` signed in with permission to dispatch workflows.

  --os NAME        linux (default, ubuntu-24.04), macos (macos-15), windows
  --artifact PATH  also bring back PATH (relative to the checkout) into
                   target/remote/<run>/files; repeatable
  --keep           keep the remote/<sha> branch

Options go before the cargo arguments; everything from the first argument
that is not one of them (or after `--`) is cargo's.

  cargo xtask remote nextest run -p fission-pcode
  cargo xtask remote --os windows -- check -p fission-cli";

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
        Some("cleanup") => match cleanup::Options::parse(&args[1..]) {
            Ok(options) => cleanup::run(&options),
            Err(message) => {
                eprintln!("error: {message}\n\n{USAGE}");
                ExitCode::from(2)
            }
        },
        Some("remote") => match remote::Options::parse(&args[1..]) {
            Ok(options) => remote::run(&options),
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
