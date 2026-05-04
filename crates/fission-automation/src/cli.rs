//! Clap CLI definitions for `fission-automation`.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "fission-automation")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Canonical automation runner for Fission quality pipelines")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    NirCheck(NirCheckArgs),
}

#[derive(Parser, Debug)]
pub struct NirCheckArgs {
    #[arg(long, default_value = "pdb")]
    pub lane: String,
    #[arg(long)]
    pub release: bool,
    #[arg(long)]
    pub no_build: bool,
    #[arg(long)]
    pub fission_bin: Option<PathBuf>,
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    #[arg(long)]
    pub baseline: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    pub update_latest: bool,
    /// Skip copying this run into `benchmark/artifacts/automation/latest/<lane>/` (CI-friendly).
    #[arg(long = "no-update-latest")]
    pub no_update_latest: bool,
    #[arg(long)]
    pub dry_run: bool,
    /// Exit with non-zero status unless `go_stop_gate.decision` starts with `go_`.
    #[arg(long)]
    pub fail_on_stop: bool,
    #[arg(long, default_value_t = 1)]
    pub jobs: usize,
    #[arg(long)]
    pub timeout_ms: Option<u64>,
    #[arg(long)]
    pub functions_limit: Option<usize>,
    #[arg(long, value_enum, default_value_t = RunProfile::Mid)]
    pub run_profile: RunProfile,
    #[arg(long)]
    pub focus_top_mismatch: Option<usize>,
    /// Emit duplicate `preview_*` JSON artifacts alongside canonical `nir_*` outputs (legacy compatibility).
    #[arg(long)]
    pub emit_legacy_preview_artifacts: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RunProfile {
    Fast,
    Mid,
    Full,
}

impl RunProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            RunProfile::Fast => "fast",
            RunProfile::Mid => "mid",
            RunProfile::Full => "full",
        }
    }

    pub fn adjust_functions_limit(self, base: Option<usize>) -> Option<usize> {
        match (self, base) {
            (RunProfile::Fast, Some(v)) => Some(v.min(10).max(1)),
            (RunProfile::Fast, None) => Some(10),
            (RunProfile::Mid, v) => v,
            (RunProfile::Full, Some(v)) => Some(v.max(40)),
            (RunProfile::Full, None) => Some(40),
        }
    }

    pub fn adjust_timeout_ms(self, base: u64) -> u64 {
        match self {
            RunProfile::Fast => base.min(1_500).max(500),
            RunProfile::Mid => base,
            RunProfile::Full => base.max(10_000),
        }
    }
}
