//! Clap CLI definitions for `fission-automation`.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

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
    /// Run the source-semantic quality benchmark (canonical quality surface).
    SourceSemanticCheck(SourceSemanticCheckArgs),
    /// Alias for `source-semantic-check`.
    #[command(hide = true)]
    Check(SourceSemanticCheckArgs),
    /// Run the NIR inventory quality lane against sentinel binaries.
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

#[derive(Parser, Debug)]
pub struct SourceSemanticCheckArgs {
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
    pub baseline_dir: Option<PathBuf>,
    #[arg(long)]
    pub jobs: Option<usize>,
    #[arg(long)]
    pub timeout_sec: Option<u64>,
    #[arg(long = "function-name")]
    pub function_names: Vec<String>,
    #[arg(long = "entry-id")]
    pub entry_ids: Vec<String>,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub include_debug_decomp: bool,
    #[arg(long)]
    pub include_ghidra_reference: bool,
    #[arg(long)]
    pub ghidra_home: Option<PathBuf>,
    #[arg(long)]
    pub fail_on_stop: bool,
    #[arg(long)]
    pub no_decomp_cache: bool,
    #[arg(long)]
    pub no_behavior_cache: bool,
    #[arg(long, value_enum, default_value_t = RunProfile::Mid)]
    pub run_profile: RunProfile,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_source_semantic_check_defaults() {
        let cli = Cli::try_parse_from(["fission-automation", "source-semantic-check"])
            .expect("source semantic command parses");
        let Commands::SourceSemanticCheck(args) = cli.command else {
            panic!("expected source semantic command");
        };
        assert!(matches!(args.run_profile, RunProfile::Mid));
        assert!(args.include_debug_decomp);
        assert!(!args.include_ghidra_reference);
        assert!(!args.no_decomp_cache);
        assert!(!args.no_behavior_cache);
        assert!(args.manifest.is_none());
        assert!(args.jobs.is_none());
    }

    #[test]
    fn parses_source_semantic_check_core_flags() {
        let cli = Cli::try_parse_from([
            "fission-automation",
            "source-semantic-check",
            "--no-build",
            "--fission-bin",
            "target/release/fission_cli",
            "--function-name",
            "fibonacci",
            "--entry-id",
            "x86-64-windows-small-c-test-functions",
            "--tag",
            "smoke",
            "--jobs",
            "1",
            "--timeout-sec",
            "30",
            "--include-ghidra-reference",
            "--ghidra-home",
            "vendor/ghidra/ghidra_12.0.4_PUBLIC",
            "--no-decomp-cache",
            "--no-behavior-cache",
        ])
        .expect("source semantic flags parse");
        let Commands::SourceSemanticCheck(args) = cli.command else {
            panic!("expected source semantic command");
        };
        assert!(args.no_build);
        assert_eq!(args.function_names, vec!["fibonacci"]);
        assert_eq!(
            args.entry_ids,
            vec!["x86-64-windows-small-c-test-functions"]
        );
        assert_eq!(args.tags, vec!["smoke"]);
        assert_eq!(args.jobs, Some(1));
        assert_eq!(args.timeout_sec, Some(30));
        assert!(args.include_debug_decomp);
        assert!(args.include_ghidra_reference);
        assert!(args.no_decomp_cache);
        assert!(args.no_behavior_cache);
        assert_eq!(
            args.ghidra_home,
            Some(PathBuf::from("vendor/ghidra/ghidra_12.0.4_PUBLIC"))
        );
    }

    #[test]
    fn check_alias_parses_as_source_semantic() {
        let cli = Cli::try_parse_from(["fission-automation", "check"]).expect("check alias parses");
        assert!(
            matches!(cli.command, Commands::Check(_)),
            "expected Check variant"
        );
    }
}
