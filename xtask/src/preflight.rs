//! `cargo xtask preflight`: the CI Fast Gate, run locally with CI's commands.
//!
//! What to run comes from CI's own classifiers: `scripts/ci/change_plan.py`
//! (which jobs a change selects) and `scripts/ci/select_affected_crates.py`
//! (which packages, reverse dependents included). How to run it comes from
//! the scripts the workflow steps call and the values the workflow files
//! pass them. Nothing here restates a CI command; the drift tests below
//! check that each check names the step that runs it and that the step still
//! runs exactly that.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

use crate::workflow;

pub const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
pub const LINT_WORKFLOW: &str = ".github/workflows/reusable-lint-format.yml";
pub const TEST_WORKFLOW: &str = ".github/workflows/reusable-run-tests.yml";

/// Test-semantics variables the test workflow sets for every step. Build and
/// cache tuning there (`CARGO_INCREMENTAL=0`, net retries) is left out on
/// purpose: it changes how fast a local build is, not what a test checks.
pub const TEST_ENV_KEYS: [&str; 2] = ["FISSION_REQUIRE_TEST_INPUTS", "RUST_BACKTRACE"];

/// Optional-feature suites: the selector flag that enables each, the
/// `scripts/ci/feature_suite.sh` argument, and the test-workflow step.
pub const FEATURE_SUITES: [(&str, &str, &str); 5] = [
    (
        "run_interactive_runtime_tests",
        "interactive_runtime",
        "Test optional features (interactive_runtime)",
    ),
    (
        "run_script_emulator_tests",
        "script_emulator",
        "Test optional features (script emulator)",
    ),
    (
        "run_emulator_softfloat_check",
        "emulator_softfloat",
        "Check optional feature (emulator softfloat)",
    ),
    (
        "run_plugin_interactive_check",
        "plugin_interactive",
        "Check optional feature (plugin interactive_runtime)",
    ),
    (
        "run_cli_alt_feature_checks",
        "cli_alt",
        "Check alternate CLI feature configurations",
    ),
];

/// CI jobs `change_plan.py` can select that preflight does not reproduce,
/// with the reason. Listed after the table so a green preflight is not read
/// as a green CI.
const NOT_LOCAL: [(&str, &str); 14] = [
    ("run_doc_checks", "Documentation examples and schemas"),
    ("run_replay_checks", "Native fixture replay and Python port"),
    ("run_security", "Security Check (cargo deny / audit)"),
    ("run_actionlint", "actionlint + zizmor"),
    ("run_debugger_tests", "Native Debugger Tests (Linux)"),
    ("run_cli_smoke", "CLI Smoke (Linux, PE sample)"),
    ("run_macos_test", "Tests (macOS)"),
    ("run_platform_debugger", "pre-merge native-debugger lane"),
    ("run_solver_diff", "pre-merge solver lane"),
    ("run_loader_fixtures", "pre-merge loader lane"),
    ("run_decomp_corpus", "pre-merge decompiler corpus lane"),
    ("run_package_check", "Package candidate rehearsal"),
    ("run_sweep_smoke", "DecBench sweep smoke"),
    ("run_sccache_fallback", "sccache outage fallback"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
    Release,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub fast: bool,
    pub full: bool,
    pub profile: Profile,
    pub base: String,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut options = Options {
            fast: false,
            full: false,
            profile: Profile::Debug,
            base: "origin/main".to_string(),
        };
        let mut profile_set = None;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--fast" => options.fast = true,
                "--full" => options.full = true,
                "--debug" | "--release" => {
                    let profile = if arg == "--debug" {
                        Profile::Debug
                    } else {
                        Profile::Release
                    };
                    if profile_set.is_some_and(|p| p != profile) {
                        return Err("--debug and --release are mutually exclusive".into());
                    }
                    profile_set = Some(profile);
                    options.profile = profile;
                }
                "--base" => {
                    options.base = iter.next().ok_or("--base needs a git ref")?.to_string();
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(options)
    }
}

/// Values preflight takes from the workflow files at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiConfig {
    /// `jobs.lint-format.with.exclude_crates` in ci.yml.
    pub clippy_exclude: String,
    /// `jobs.test-linux.with.retries` in ci.yml, else `inputs.retries.default`
    /// in the test workflow.
    pub retries: String,
    /// [`TEST_ENV_KEYS`] as the test workflow sets them.
    pub test_env: Vec<(String, String)>,
}

impl CiConfig {
    pub fn load(root: &Path) -> Result<CiConfig, String> {
        let read =
            |rel: &str| std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"));
        let ci = read(CI_WORKFLOW)?;
        let tests = read(TEST_WORKFLOW)?;
        let clippy_exclude = workflow::job_input(&ci, "lint-format", "exclude_crates")
            .map_err(|e| format!("{CI_WORKFLOW}: {e}"))?;
        // What the Linux test job passes, else the reusable workflow's default.
        let retries = match workflow::job_input(&ci, "test-linux", "retries") {
            Ok(value) => value,
            Err(_) => workflow::input_default(&tests, "retries")
                .map_err(|e| format!("{TEST_WORKFLOW}: {e}"))?,
        };
        let env = workflow::workflow_env(&tests).map_err(|e| format!("{TEST_WORKFLOW}: {e}"))?;
        let mut test_env = Vec::new();
        for key in TEST_ENV_KEYS {
            let value = env
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("{TEST_WORKFLOW}: env {key} not set"))?;
            test_env.push((key.to_string(), value));
        }
        Ok(CiConfig {
            clippy_exclude,
            retries,
            test_env,
        })
    }
}

/// What CI's classifiers selected for the change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub lane: String,
    /// `change_plan.py` job flags.
    pub jobs: BTreeMap<String, bool>,
    /// `select_affected_crates.py` packages; empty when no tests run.
    pub crates: Vec<String>,
    /// `select_affected_crates.py` feature-suite flags.
    pub suites: BTreeMap<String, bool>,
}

impl Selection {
    pub fn job(&self, name: &str) -> bool {
        self.jobs.get(name).copied().unwrap_or(false)
    }
}

/// Parse the `key=value` lines CI's classifiers print for `$GITHUB_OUTPUT`.
pub fn parse_outputs(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn flags(map: &BTreeMap<String, String>) -> BTreeMap<String, bool> {
    map.iter()
        .filter(|(k, _)| k.starts_with("run_"))
        .map(|(k, v)| (k.clone(), v == "true"))
        .collect()
}

/// `change_plan.py --classify` output -> lane and job flags.
pub fn parse_plan(text: &str) -> Result<(String, BTreeMap<String, bool>), String> {
    let map = parse_outputs(text);
    let lane = map
        .get("lane")
        .cloned()
        .ok_or("change_plan.py printed no lane")?;
    Ok((lane, flags(&map)))
}

/// `select_affected_crates.py` output -> sorted packages and suite flags.
pub fn parse_crates(text: &str) -> Result<(Vec<String>, BTreeMap<String, bool>), String> {
    let map = parse_outputs(text);
    let crates = map
        .get("crates")
        .ok_or("select_affected_crates.py printed no crates")?;
    let mut crates: Vec<String> = crates
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect();
    crates.sort();
    crates.dedup();
    Ok((crates, flags(&map)))
}

/// Where CI runs a check, and the line in that step's `run:` that runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiRef {
    pub workflow: &'static str,
    pub step: &'static str,
    pub invocation: String,
    /// Whether every `env:` key of the step must be one preflight sets. Off
    /// for the targeted-test steps: `BASE_REF` feeds CI's own fetch (preflight
    /// uses the merge base) and `enforce` gets `collect`'s outputs.
    pub check_env: bool,
    /// Other command lines the step may run besides `invocation` (CI-only
    /// setup preflight does differently). Any other line is drift: the step
    /// would run something preflight does not.
    pub setup: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Plain,
    /// `targeted_test_gate.sh collect BASE`, then `enforce` with its outputs.
    TargetedTests,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub ci: Vec<CiRef>,
    /// An external tool without which the check would pass vacuously.
    pub requires: Option<&'static str>,
    /// Reported, but not counted in the exit status: no CI step runs it.
    pub advisory: bool,
    pub kind: Kind,
}

fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:,+@%".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

impl Check {
    fn new(name: &'static str, program: &str, args: &[&str]) -> Check {
        Check {
            name,
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: Vec::new(),
            ci: Vec::new(),
            requires: None,
            advisory: false,
            kind: Kind::Plain,
        }
    }

    fn env(mut self, key: &str, value: &str) -> Check {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    fn envs(mut self, pairs: &[(String, String)]) -> Check {
        self.env.extend(pairs.iter().cloned());
        self
    }

    /// CI runs this exact command line in `step` of `workflow`.
    fn in_ci(mut self, workflow: &'static str, step: &'static str) -> Check {
        let invocation = self.command_line();
        self.ci.push(CiRef {
            workflow,
            step,
            invocation,
            check_env: true,
            setup: Vec::new(),
        });
        self
    }

    /// The command line without its environment, shell-quoted.
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(quote)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The command line as printed before it runs, environment first.
    pub fn display(&self) -> String {
        let mut out = String::new();
        for (k, v) in &self.env {
            let _ = write!(out, "{k}={} ", quote(v));
        }
        out.push_str(&self.command_line());
        out
    }
}

/// The ordered list of checks for a selection. Pure: the same inputs always
/// give the same list.
pub fn plan(options: &Options, ci: &CiConfig, selection: &Selection, base: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    let crates = selection.crates.join(",");

    if selection.job("run_lint") {
        checks.push(
            Check::new("fmt", "cargo", &["fmt", "--all", "--check"])
                .in_ci(LINT_WORKFLOW, "Check Rust formatting"),
        );
        checks.push(
            Check::new(
                "benchmark-fixture-refs",
                "python3",
                &["scripts/test/no_benchmark_fixture_refs_in_crates.py"],
            )
            .in_ci(
                LINT_WORKFLOW,
                "Guard — crate sources must not reference benchmark fixtures",
            ),
        );
        // --full lints with the command a push to main runs (no package
        // list: `--workspace`), the one that caught what `-p` missed.
        let clippy_crates = if options.full { "" } else { crates.as_str() };
        checks.push(
            Check::new("clippy", "bash", &["scripts/ci/clippy.sh"])
                .env("CRATES", clippy_crates)
                .env("EXCLUDE_CRATES", &ci.clippy_exclude)
                .in_ci(LINT_WORKFLOW, "Lint with Clippy (Deny Warnings)"),
        );
        checks.push(
            Check::new("unrun-tests", "scripts/audit/unrun_tests_scan.py", &[])
                .in_ci(LINT_WORKFLOW, "Tests nothing runs"),
        );
    }

    if selection.job("run_pass_gate") {
        checks.push(
            Check::new(
                "pass-gate-patterns",
                "bash",
                &["scripts/check/pass_gate_patterns.sh"],
            )
            .in_ci(
                CI_WORKFLOW,
                "Check for function/address-specific patterns in NIR pipeline",
            ),
        );
        checks.push(
            Check::new(
                "max-structuring-passes",
                "bash",
                &["scripts/check/max_structuring_passes.sh"],
            )
            .in_ci(
                CI_WORKFLOW,
                "Verify MAX_STRUCTURING_PASSES constant is declared",
            ),
        );
        let mut owner = Check::new(
            "owner-boundaries",
            "./scripts/check/owner_boundaries.sh",
            &[],
        )
        .in_ci(CI_WORKFLOW, "Owner boundary scan (ADR 0008 / 0011 / 0012)");
        owner.requires = Some("rg");
        checks.push(owner);
        // AGENTS.md names this as a CI check, but no workflow runs it, and it
        // fails on main. Report it without failing preflight: preflight's
        // exit status means "CI would pass".
        let mut literals = Check::new(
            "resource-path-literals",
            "bash",
            &["scripts/test/check_resource_path_literals.sh"],
        );
        literals.requires = Some("rg");
        literals.advisory = true;
        checks.push(literals);
    }

    // CI enforces this on pull requests whenever tests run.
    if selection.job("run_pass_gate") && selection.job("run_tests") {
        let mut targeted = Check::new(
            "targeted-tests",
            "bash",
            &["scripts/ci/targeted_test_gate.sh", "collect", base],
        )
        .in_ci(CI_WORKFLOW, "Collect PR changes for scoped gates");
        targeted.ci.push(CiRef {
            workflow: CI_WORKFLOW,
            step: "Enforce Targeted Tests for NIR/Render Changes",
            invocation: "bash scripts/ci/targeted_test_gate.sh enforce".to_string(),
            check_env: false,
            setup: Vec::new(),
        });
        targeted.ci[0].check_env = false;
        // CI fetches the base tip itself; preflight passes the merge base.
        targeted.ci[0].setup = vec![
            r#"git fetch --no-tags origin "$BASE_REF" --depth=1"#,
            "BASE_SHA=$(git rev-parse FETCH_HEAD)",
        ];
        // CI passes the fetched base tip as "$BASE_SHA"; preflight passes the
        // merge base. Compare the command with CI's spelling of the argument.
        let mut ci_args = targeted.clone();
        ci_args.args[2] = "$BASE_SHA".to_string();
        targeted.ci[0].invocation = ci_args.command_line();
        targeted.kind = Kind::TargetedTests;
        checks.push(targeted);
    }

    if selection.job("run_script_checks") {
        checks.push(
            Check::new(
                "script-syntax-py",
                "bash",
                &["scripts/ci/script_syntax.sh", "py"],
            )
            .in_ci(CI_WORKFLOW, "Python syntax (scripts/)"),
        );
        checks.push(
            Check::new(
                "script-syntax-sh",
                "bash",
                &["scripts/ci/script_syntax.sh", "sh"],
            )
            .in_ci(CI_WORKFLOW, "Shell syntax (scripts/**/*.sh)"),
        );
        checks.push(
            Check::new(
                "ci-script-tests",
                "python3",
                &[
                    "-m",
                    "unittest",
                    "discover",
                    "-s",
                    "scripts/ci",
                    "-p",
                    "test_*.py",
                ],
            )
            .in_ci(CI_WORKFLOW, "CI classifier and tooling tests"),
        );
    }

    if selection.job("run_tests") && !options.fast && !selection.crates.is_empty() {
        let profile = options.profile.as_str();
        checks.push(
            Check::new("nextest", "bash", &["scripts/ci/test_crates.sh", "nextest"])
                .envs(&ci.test_env)
                // Same nextest profile as CI: the default profile plus the
                // JUnit report the Sentry defect reporter reads.
                .env("NEXTEST_PROFILE", "ci")
                .env("CRATES", &crates)
                .env("FEATURES", "")
                .env("RETRIES", &ci.retries)
                .env("PROFILE", profile)
                .in_ci(TEST_WORKFLOW, "Run Tests (Specific Crates) [nextest]"),
        );
        checks.push(
            Check::new("doctests", "bash", &["scripts/ci/test_crates.sh", "doc"])
                .envs(&ci.test_env)
                .env("CRATES", &crates)
                .env("FEATURES", "")
                .env("PROFILE", profile)
                .in_ci(TEST_WORKFLOW, "Run Doctests (Specific Crates)"),
        );
        for (flag, suite, step) in FEATURE_SUITES {
            if selection.suites.get(flag).copied().unwrap_or(false) {
                checks.push(
                    Check::new(suite, "bash", &["scripts/ci/feature_suite.sh", suite])
                        .envs(&ci.test_env)
                        .in_ci(TEST_WORKFLOW, step),
                );
            }
        }
    }
    checks
}

// ---------------------------------------------------------------------------
// Drift: a check against the workflow step that runs it in CI

/// Lines of a step's `run:` that are commands, not shell setup.
fn commands(step: &workflow::Step) -> Vec<String> {
    step.run
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with('#')
                && l != "set -euo pipefail"
                && !l.starts_with("chmod +x ")
        })
        .collect()
}

fn unquote(s: &str) -> String {
    s.replace(['\'', '"'], "")
}

/// Every way `check` can disagree with the step CI runs it in.
pub fn drift(check: &Check, files: &BTreeMap<&str, String>) -> Vec<String> {
    let mut problems = Vec::new();
    for r in &check.ci {
        let step = match workflow::step(&files[r.workflow], r.step) {
            Ok(step) => step,
            Err(e) => {
                problems.push(format!("{}: {}: {e}", check.name, r.workflow));
                continue;
            }
        };
        let want = unquote(&r.invocation);
        let is_invocation = |line: &str| {
            let line = unquote(line);
            line == want
                || line
                    .strip_prefix(&want)
                    .is_some_and(|rest| rest.starts_with(" >>"))
        };
        let lines = commands(&step);
        let runs = lines.iter().any(|line| is_invocation(line));
        // Lines beside the invocation (a changed invocation is reported
        // once, below, with the whole `run:`).
        for line in lines.iter().filter(|_| runs) {
            if !is_invocation(line) && !r.setup.contains(&line.as_str()) {
                problems.push(format!(
                    "{}: `{}` / `{}` also runs `{line}`, which preflight does not",
                    check.name, r.workflow, r.step
                ));
            }
        }
        if step.continue_on_error {
            problems.push(format!(
                "{}: `{}` / `{}` has continue-on-error, so its failure no longer fails CI",
                check.name, r.workflow, r.step
            ));
        }
        if !runs {
            problems.push(format!(
                "{}: `{}` / `{}` runs {:?}, preflight runs `{}`",
                check.name, r.workflow, r.step, step.run, r.invocation
            ));
        }
        for (key, _) in &step.env {
            if r.check_env && !check.env.iter().any(|(k, _)| k == key) {
                problems.push(format!(
                    "{}: CI step `{}` sets {key}, preflight does not",
                    check.name, r.step
                ));
            }
        }
    }
    problems
}

// ---------------------------------------------------------------------------
// Running

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn python(root: &Path, args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    let mut child = Command::new("python3")
        .args(args)
        .current_dir(root)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("python3: {e}"))?;
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(input.as_bytes())
            .map_err(|e| format!("python3 stdin: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("python3: {e}"))?;
    if !out.status.success() {
        return Err(format!("python3 {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Changed paths and the merge base, as `change_plan.py` lists them for a
/// pull request (renames as a removal and an addition).
fn changed_paths(root: &Path, base: &str) -> Result<(String, Vec<String>), String> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])?.trim().to_string();
    let diff = git(
        root,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &merge_base,
            "HEAD",
        ],
    )?;
    let paths = diff
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    Ok((merge_base, paths))
}

fn select(root: &Path, paths: &[String], event: &str) -> Result<Selection, String> {
    let list = paths.join("\n") + "\n";
    let file = std::env::temp_dir().join(format!("xtask-preflight-{}.txt", std::process::id()));
    std::fs::write(&file, &list).map_err(|e| format!("{}: {e}", file.display()))?;
    let plan_out = python(
        root,
        &[
            "scripts/ci/change_plan.py",
            "--classify",
            &file.to_string_lossy(),
            "--event",
            event,
        ],
        None,
    );
    let _ = std::fs::remove_file(&file);
    let (lane, jobs) = parse_plan(&plan_out?)?;
    let mut selection = Selection {
        lane,
        jobs,
        ..Selection::default()
    };
    // CI resolves packages only when tests run; lint then reuses the list.
    if selection.job("run_tests") {
        let out = python(root, &["scripts/ci/select_affected_crates.py"], Some(&list))?;
        let (crates, suites) = parse_crates(&out)?;
        selection.crates = crates;
        selection.suites = suites;
    }
    Ok(selection)
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn spawn(check: &Check, args: &[String], extra_env: &[(String, String)]) -> Result<bool, String> {
    let status = Command::new(&check.program)
        .args(args)
        .envs(check.env.iter().cloned())
        .envs(extra_env.iter().cloned())
        .status()
        .map_err(|e| format!("{}: {e}", check.program))?;
    Ok(status.success())
}

/// Run one check. `Err` carries a reason that is not the command's own output.
fn execute(check: &Check) -> Result<bool, String> {
    if let Some(tool) = check.requires {
        if !tool_available(tool) {
            return Err(format!(
                "`{tool}` is not on PATH; the script would pass without scanning anything"
            ));
        }
    }
    match check.kind {
        Kind::Plain => spawn(check, &check.args, &[]),
        Kind::TargetedTests => {
            let out = Command::new(&check.program)
                .args(&check.args)
                .stderr(Stdio::inherit())
                .output()
                .map_err(|e| format!("{}: {e}", check.program))?;
            if !out.status.success() {
                return Ok(false);
            }
            let outputs = parse_outputs(&String::from_utf8_lossy(&out.stdout));
            let env: Vec<(String, String)> = [
                ("NIR_RENDER_MIDEND_CHANGED", "nir_render_midend_changed"),
                ("HAS_TARGETED_TEST", "has_targeted_test"),
            ]
            .iter()
            .map(|(var, key)| {
                (
                    var.to_string(),
                    outputs.get(*key).cloned().unwrap_or_default(),
                )
            })
            .collect();
            println!("+ bash scripts/ci/targeted_test_gate.sh enforce");
            spawn(
                check,
                &[
                    "scripts/ci/targeted_test_gate.sh".to_string(),
                    "enforce".to_string(),
                ],
                &env,
            )
        }
    }
}

struct Outcome {
    name: &'static str,
    passed: bool,
    seconds: f64,
    note: String,
}

fn repo_root() -> Result<PathBuf, String> {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !top.status.success() {
        return Err("not inside a git checkout".into());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&top.stdout).trim()))
}

pub fn run(options: &Options) -> ExitCode {
    match run_inner(options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("preflight: {message}");
            ExitCode::from(2)
        }
    }
}

fn run_inner(options: &Options) -> Result<bool, String> {
    let root = repo_root()?;
    std::env::set_current_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let ci = CiConfig::load(&root)?;

    let (merge_base, paths, event) = if options.full {
        // What a workflow_dispatch run checks: everything.
        let merge_base = git(&root, &["merge-base", &options.base, "HEAD"])?
            .trim()
            .to_string();
        (
            merge_base,
            vec!["Cargo.toml".to_string()],
            "workflow_dispatch",
        )
    } else {
        let (merge_base, paths) = changed_paths(&root, &options.base)?;
        (merge_base, paths, "pull_request")
    };
    if paths.is_empty() {
        println!(
            "preflight: no commits on HEAD since {} ({}); nothing to check. \
             Commit first, or pass --full.",
            options.base,
            &merge_base[..merge_base.len().min(12)]
        );
        return Ok(true);
    }
    let dirty = git(&root, &["status", "--porcelain", "--untracked-files=no"])?;
    let selection = select(&root, &paths, event)?;
    let checks = plan(options, &ci, &selection, &merge_base);
    let files: BTreeMap<&str, String> = [CI_WORKFLOW, LINT_WORKFLOW, TEST_WORKFLOW]
        .into_iter()
        .map(|f| std::fs::read_to_string(root.join(f)).map(|text| (f, text)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading workflows: {e}"))?;
    let problems: Vec<String> = checks.iter().flat_map(|c| drift(c, &files)).collect();
    if !problems.is_empty() {
        return Err(format!(
            "these checks no longer match the CI steps that run them; update xtask:\n{}",
            problems.join("\n")
        ));
    }

    println!(
        "preflight: base {} (merge-base {}), {} changed path(s), event {}, lane {}",
        options.base,
        &merge_base[..merge_base.len().min(12)],
        if options.full { 0 } else { paths.len() },
        event,
        selection.lane
    );
    if !selection.crates.is_empty() {
        println!(
            "preflight: affected packages (with reverse dependents, as CI selects them): {}",
            selection.crates.join(", ")
        );
    }
    if !dirty.trim().is_empty() {
        println!(
            "preflight: note: uncommitted changes are checked here, but CI's change \
             selection sees only commits"
        );
    }

    let mut outcomes = Vec::new();
    for (i, check) in checks.iter().enumerate() {
        println!(
            "\n==> [{}/{}] {}: {}",
            i + 1,
            checks.len(),
            check.name,
            check.display()
        );
        let start = Instant::now();
        let result = execute(check);
        let seconds = start.elapsed().as_secs_f64();
        let (passed, note) = match result {
            Ok(passed) => (passed, String::new()),
            Err(reason) => {
                println!("preflight: {}: {reason}", check.name);
                (false, reason)
            }
        };
        outcomes.push(Outcome {
            name: check.name,
            passed,
            seconds,
            note,
        });
    }

    print!("{}", summary(&checks, &outcomes));
    let skipped: Vec<&str> = NOT_LOCAL
        .iter()
        .filter(|(flag, _)| selection.job(flag))
        .map(|(_, job)| *job)
        .collect();
    if !skipped.is_empty() {
        println!(
            "\nCI also selects (not run by preflight): {}",
            skipped.join("; ")
        );
    }
    if options.fast && selection.job("run_tests") {
        println!("--fast: nextest, doctests and feature suites skipped.");
    }
    Ok(all_passed(&checks, &outcomes))
}

/// The exit status: every check that CI also runs passed.
fn all_passed(checks: &[Check], outcomes: &[Outcome]) -> bool {
    checks
        .iter()
        .zip(outcomes)
        .all(|(c, o)| o.passed || c.advisory)
}

fn summary(checks: &[Check], outcomes: &[Outcome]) -> String {
    let mut out = String::from("\n| check | result | time | CI step |\n|---|---|---|---|\n");
    for (check, outcome) in checks.iter().zip(outcomes) {
        let step = match check.ci.first() {
            Some(r) => r.step.to_string(),
            None => "local only".to_string(),
        };
        let result = match (outcome.passed, check.advisory) {
            (true, _) => "PASS",
            (false, false) => "FAIL",
            (false, true) => "WARN (advisory: no CI step runs it)",
        };
        let note = if outcome.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", outcome.note)
        };
        let _ = writeln!(
            out,
            "| {} | {}{} | {:.1}s | {} |",
            outcome.name, result, note, outcome.seconds, step
        );
    }
    let failed = checks
        .iter()
        .zip(outcomes)
        .filter(|(c, o)| !o.passed && !c.advisory)
        .count();
    let _ = writeln!(
        out,
        "\npreflight: {} check(s), {} failed",
        outcomes.len(),
        failed
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask sits in the workspace root")
            .to_path_buf()
    }

    fn ci_config() -> CiConfig {
        CiConfig {
            clippy_exclude: "fission-signatures".into(),
            retries: "1".into(),
            test_env: vec![("FISSION_REQUIRE_TEST_INPUTS".into(), "1".into())],
        }
    }

    fn options() -> Options {
        Options::parse(&[]).unwrap()
    }

    fn selection(jobs: &[&str], crates: &[&str], suites: &[&str]) -> Selection {
        Selection {
            lane: "rust".into(),
            jobs: jobs.iter().map(|j| (j.to_string(), true)).collect(),
            crates: crates.iter().map(|c| c.to_string()).collect(),
            suites: suites.iter().map(|s| (s.to_string(), true)).collect(),
        }
    }

    fn names(checks: &[Check]) -> Vec<&'static str> {
        checks.iter().map(|c| c.name).collect()
    }

    // --- options -----------------------------------------------------------

    #[test]
    fn options_default_to_debug_against_origin_main() {
        let o = options();
        assert_eq!(o.profile, Profile::Debug);
        assert_eq!(o.base, "origin/main");
        assert!(!o.fast && !o.full);
        let o = Options::parse(&["--release".into(), "--base".into(), "HEAD~1".into()]).unwrap();
        assert_eq!(o.profile, Profile::Release);
        assert_eq!(o.base, "HEAD~1");
        assert!(Options::parse(&["--debug".into(), "--release".into()]).is_err());
        assert!(Options::parse(&["--bogus".into()]).is_err());
    }

    // --- path -> crate mapping (CI's selector output) -----------------------

    #[test]
    fn parses_the_selectors_github_output() {
        let (crates, suites) = parse_crates(
            "crates=fission-pcode,fission-cli,fission-pcode\nrun_cli_smoke=true\n\
             run_cli_alt_feature_checks=true\nrun_script_emulator_tests=false\n",
        )
        .unwrap();
        assert_eq!(
            crates,
            vec!["fission-cli", "fission-pcode"],
            "sorted, deduplicated"
        );
        assert_eq!(suites.get("run_cli_alt_feature_checks"), Some(&true));
        assert_eq!(suites.get("run_script_emulator_tests"), Some(&false));
        assert!(parse_crates("run_cli_smoke=true\n").is_err());
        let (lane, jobs) = parse_plan("lane=docs\nrun_lint=false\nrun_pass_gate=true\n").unwrap();
        assert_eq!(lane, "docs");
        assert_eq!(jobs.get("run_pass_gate"), Some(&true));
        assert!(parse_plan("run_lint=true\n").is_err());
    }

    /// The real classifiers on real paths: a leaf crate pulls in its reverse
    /// dependents (positive control), a docs edit selects no tests, and this
    /// crate is its own owner.
    #[test]
    fn real_classifiers_map_paths_to_crates() {
        let root = root();
        let ir = select(
            &root,
            &["crates/fission-pcode-ir/src/lib.rs".into()],
            "pull_request",
        )
        .expect("python3 and cargo are required to run the classifiers");
        assert_eq!(ir.lane, "rust");
        assert!(ir.job("run_tests") && ir.job("run_lint"));
        for expected in [
            "fission-pcode-ir",
            "fission-pcode",
            "fission-decompiler",
            "fission-cli",
        ] {
            assert!(
                ir.crates.iter().any(|c| c == expected),
                "{expected} in {:?}",
                ir.crates
            );
        }

        let docs = select(&root, &["docs/REVIEW_POLICY.md".into()], "pull_request").unwrap();
        assert_eq!(docs.lane, "docs");
        assert!(!docs.job("run_tests") && !docs.job("run_lint"));
        assert!(docs.crates.is_empty());

        let me = select(&root, &["xtask/src/preflight.rs".into()], "pull_request").unwrap();
        assert_eq!(me.crates, vec!["xtask"]);
    }

    // --- command list -------------------------------------------------------

    #[test]
    fn rust_change_runs_lint_pass_gate_and_tests_in_order() {
        let sel = selection(
            &["run_lint", "run_pass_gate", "run_tests"],
            &["fission-cli", "fission-pcode"],
            &["run_cli_alt_feature_checks"],
        );
        let checks = plan(&options(), &ci_config(), &sel, "abc");
        assert_eq!(
            names(&checks),
            vec![
                "fmt",
                "benchmark-fixture-refs",
                "clippy",
                "unrun-tests",
                "pass-gate-patterns",
                "max-structuring-passes",
                "owner-boundaries",
                "resource-path-literals",
                "targeted-tests",
                "nextest",
                "doctests",
                "cli_alt",
            ]
        );
        let clippy = &checks[2];
        assert_eq!(
            clippy.display(),
            "CRATES=fission-cli,fission-pcode EXCLUDE_CRATES=fission-signatures bash scripts/ci/clippy.sh"
        );
        let nextest = checks.iter().find(|c| c.name == "nextest").unwrap();
        assert!(
            nextest.env.contains(&("PROFILE".into(), "debug".into())),
            "debug by default"
        );
        assert!(
            nextest
                .env
                .contains(&("FISSION_REQUIRE_TEST_INPUTS".into(), "1".into()))
        );
        assert!(
            nextest
                .env
                .contains(&("NEXTEST_PROFILE".into(), "ci".into())),
            "same nextest profile as the CI step"
        );
        // Same inputs, same list.
        assert_eq!(checks, plan(&options(), &ci_config(), &sel, "abc"));
    }

    #[test]
    fn fast_skips_tests_and_full_lints_the_workspace() {
        let sel = selection(
            &["run_lint", "run_pass_gate", "run_tests"],
            &["a", "b"],
            &[],
        );
        let mut o = options();
        o.fast = true;
        let fast = names(&plan(&o, &ci_config(), &sel, "x"));
        assert!(!fast.contains(&"nextest") && !fast.contains(&"doctests"));
        assert!(fast.contains(&"clippy"), "--fast still lints");

        o.fast = false;
        o.full = true;
        o.profile = Profile::Release;
        let full = plan(&o, &ci_config(), &sel, "x");
        let clippy = full.iter().find(|c| c.name == "clippy").unwrap();
        assert!(
            clippy.env.contains(&("CRATES".into(), String::new())),
            "empty CRATES = --workspace"
        );
        let nextest = full.iter().find(|c| c.name == "nextest").unwrap();
        assert!(nextest.env.contains(&("CRATES".into(), "a,b".into())));
        assert!(nextest.env.contains(&("PROFILE".into(), "release".into())));
    }

    #[test]
    fn docs_lane_runs_nothing_and_scripts_lane_runs_script_checks() {
        // Negative: no job selected, no check.
        assert!(plan(&options(), &ci_config(), &selection(&[], &[], &[]), "x").is_empty());
        // Positive control: the same call with the scripts flags set.
        let checks = plan(
            &options(),
            &ci_config(),
            &selection(&["run_pass_gate", "run_script_checks"], &[], &[]),
            "x",
        );
        let n = names(&checks);
        assert!(n.contains(&"script-syntax-py") && n.contains(&"ci-script-tests"));
        assert!(
            !n.contains(&"targeted-tests"),
            "CI enforces it only when tests run"
        );
        assert!(!n.contains(&"nextest"));
    }

    #[test]
    fn an_advisory_failure_is_reported_but_does_not_fail_preflight() {
        let sel = selection(&["run_pass_gate"], &[], &[]);
        let checks = plan(&options(), &ci_config(), &sel, "x");
        let outcome = |c: &Check, passed| Outcome {
            name: c.name,
            passed,
            seconds: 0.0,
            note: String::new(),
        };
        let literal = |c: &Check| c.name == "resource-path-literals";
        assert!(checks.iter().any(|c| literal(c) && c.advisory));
        let only_advisory_fails: Vec<Outcome> =
            checks.iter().map(|c| outcome(c, !literal(c))).collect();
        assert!(all_passed(&checks, &only_advisory_fails));
        let table = summary(&checks, &only_advisory_fails);
        assert!(table.contains("| resource-path-literals | WARN (advisory"));
        assert!(table.contains("0 failed"));
        // Positive control: a CI check failing fails preflight.
        let ci_fails: Vec<Outcome> = checks
            .iter()
            .map(|c| outcome(c, c.name != "owner-boundaries"))
            .collect();
        assert!(!all_passed(&checks, &ci_fails));
        assert!(summary(&checks, &ci_fails).contains("| owner-boundaries | FAIL"));
    }

    // --- drift: preflight against the workflow files ------------------------

    fn workflow_files() -> BTreeMap<&'static str, String> {
        [CI_WORKFLOW, LINT_WORKFLOW, TEST_WORKFLOW]
            .into_iter()
            .map(|f| (f, std::fs::read_to_string(root().join(f)).unwrap()))
            .collect()
    }

    fn every_check() -> Vec<Check> {
        let all_suites: Vec<&str> = FEATURE_SUITES.iter().map(|(f, _, _)| *f).collect();
        let sel = selection(
            &[
                "run_lint",
                "run_pass_gate",
                "run_tests",
                "run_script_checks",
            ],
            &["fission-pcode"],
            &all_suites,
        );
        plan(
            &options(),
            &CiConfig::load(&root()).unwrap(),
            &sel,
            "0123abcd",
        )
    }

    #[test]
    fn every_check_runs_what_its_ci_step_runs() {
        let files = workflow_files();
        let checks = every_check();
        assert_eq!(checks.len(), 19, "{:?}", names(&checks));
        let problems: Vec<String> = checks.iter().flat_map(|c| drift(c, &files)).collect();
        assert!(
            problems.is_empty(),
            "preflight and CI disagree:\n{}",
            problems.join("\n")
        );
    }

    /// Positive control for the drift check: put back the inline clippy line
    /// the Lint job ran before this crate existed, or drop an env input, and
    /// the same check must report it.
    #[test]
    fn drift_check_catches_a_diverged_clippy_step() {
        let checks = every_check();
        let clippy = checks.iter().find(|c| c.name == "clippy").unwrap();
        let mut files = workflow_files();
        assert!(drift(clippy, &files).is_empty());

        let lint = files[LINT_WORKFLOW].clone();
        files.insert(
            LINT_WORKFLOW,
            lint.replace(
                "run: bash scripts/ci/clippy.sh",
                "run: cargo clippy -p fission-pcode -- -D warnings",
            ),
        );
        assert_eq!(drift(clippy, &files).len(), 1, "a changed command is drift");

        files.insert(
            LINT_WORKFLOW,
            lint.replace(
                "          EXCLUDE_CRATES: ${{ inputs.exclude_crates }}\n",
                "          EXCLUDE_CRATES: ${{ inputs.exclude_crates }}\n          EXTRA: x\n",
            ),
        );
        assert_eq!(drift(clippy, &files).len(), 1, "a new env input is drift");

        // The step keeps the script line but also runs something else.
        files.insert(
            LINT_WORKFLOW,
            lint.replace(
                "        run: bash scripts/ci/clippy.sh\n",
                "        run: |\n          bash scripts/ci/clippy.sh\n          cargo clippy --workspace\n",
            ),
        );
        assert_ne!(files[LINT_WORKFLOW], lint);
        assert_eq!(drift(clippy, &files).len(), 1, "an extra command is drift");

        // The step still runs the script, but its failure no longer fails CI.
        files.insert(
            LINT_WORKFLOW,
            lint.replace(
                "      - name: Lint with Clippy (Deny Warnings)\n",
                "      - name: Lint with Clippy (Deny Warnings)\n        continue-on-error: true\n",
            ),
        );
        assert_ne!(files[LINT_WORKFLOW], lint);
        assert_eq!(drift(clippy, &files).len(), 1, "continue-on-error is drift");
    }

    /// The targeted-test step's CI-only fetch is allowed; anything else added
    /// to it is drift.
    #[test]
    fn drift_check_allows_only_the_declared_setup_lines() {
        let checks = every_check();
        let targeted = checks.iter().find(|c| c.name == "targeted-tests").unwrap();
        let mut files = workflow_files();
        assert!(drift(targeted, &files).is_empty());
        let ci = files[CI_WORKFLOW].clone();
        let fetch = "          BASE_SHA=$(git rev-parse FETCH_HEAD)\n";
        assert_eq!(ci.matches(fetch).count(), 1);
        files.insert(
            CI_WORKFLOW,
            ci.replace(
                fetch,
                &format!("{fetch}          git checkout FETCH_HEAD\n"),
            ),
        );
        assert_eq!(drift(targeted, &files).len(), 1);
    }

    /// Retries are what the Linux test job passes, not only the default.
    #[test]
    fn retries_follow_what_the_test_job_passes() {
        let dir = std::env::temp_dir().join(format!("xtask-retries-{}", std::process::id()));
        let wf = dir.join(".github/workflows");
        std::fs::create_dir_all(&wf).unwrap();
        for f in [CI_WORKFLOW, LINT_WORKFLOW, TEST_WORKFLOW] {
            std::fs::copy(root().join(f), dir.join(f)).unwrap();
        }
        let default = CiConfig::load(&dir).unwrap().retries;
        let tests = std::fs::read_to_string(root().join(TEST_WORKFLOW)).unwrap();
        assert_eq!(default, workflow::input_default(&tests, "retries").unwrap());

        let ci = std::fs::read_to_string(root().join(CI_WORKFLOW)).unwrap();
        let anchor =
            "      crates: ${{ needs.changes.outputs.test_crates }}\n      profile: debug\n";
        assert!(ci.contains(anchor));
        let passed = ci.replacen(anchor, &format!("{anchor}      retries: 7\n"), 2);
        std::fs::write(dir.join(CI_WORKFLOW), passed).unwrap();
        assert_eq!(CiConfig::load(&dir).unwrap().retries, "7");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The clippy command itself: the Lint job's two shapes (affected
    /// packages on a pull request, the workspace on a push to main), run
    /// through the script with `cargo` replaced by a recorder.
    #[cfg(unix)]
    #[test]
    fn clippy_script_runs_ci_clippy_command() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("xtask-fake-cargo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("argv");
        let fake = dir.join("cargo");
        std::fs::write(
            &fake,
            format!("#!/bin/sh\necho \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let lints = "--all-targets --locked -- -D warnings -A clippy::style -A clippy::complexity";

        let run = |crates: &str| {
            let _ = std::fs::remove_file(&log);
            let status = Command::new("bash")
                .arg(root().join("scripts/ci/clippy.sh"))
                .env("PATH", &path)
                .env("CRATES", crates)
                .env("EXCLUDE_CRATES", "fission-signatures")
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
            std::fs::read_to_string(&log)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        // Linting fission-cli also lints its opt-in `sentry` feature.
        let sentry = format!("clippy -p fission-cli --features sentry {lints}");
        assert_eq!(
            run("fission-pcode, fission-signatures,fission-cli"),
            format!("clippy -p fission-pcode -p fission-cli {lints}\n{sentry}")
        );
        assert_eq!(
            run(""),
            format!("clippy --workspace --exclude fission-signatures {lints}\n{sentry}")
        );
        assert_eq!(
            run("fission-pcode"),
            format!("clippy -p fission-pcode {lints}")
        );
        // Only excluded packages selected: nothing to lint, cargo never runs.
        assert_eq!(run("fission-signatures"), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ci_passes_the_values_preflight_reads() {
        let files = workflow_files();
        let ci = &files[CI_WORKFLOW];
        assert_eq!(
            workflow::job_input(ci, "lint-format", "crates").unwrap(),
            "${{ needs.changes.outputs.test_crates }}",
            "Lint gets the selector's package list, which preflight also uses"
        );
        assert_eq!(
            workflow::job_input(ci, "test-linux", "crates").unwrap(),
            "${{ needs.changes.outputs.test_crates }}"
        );
        assert_eq!(
            workflow::job_input(ci, "test-linux", "profile").unwrap(),
            "debug"
        );
        for (flag, _, step) in FEATURE_SUITES {
            let s = workflow::step(&files[TEST_WORKFLOW], step).unwrap();
            assert!(
                s.condition.contains(&format!("inputs.{flag}")),
                "{step} runs on {}",
                s.condition
            );
            assert!(
                workflow::job_input(ci, "test-linux", flag).is_ok(),
                "test-linux passes {flag}"
            );
        }
        let config = CiConfig::load(&root()).unwrap();
        assert!(!config.clippy_exclude.is_empty());
        assert_eq!(config.test_env.len(), TEST_ENV_KEYS.len());
    }
}
