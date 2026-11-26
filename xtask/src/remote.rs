//! `cargo xtask remote`: run one cargo command on a hosted runner.
//!
//! The working tree, uncommitted and untracked files included (`.gitignore`
//! respected), becomes a commit that is pushed as a temporary
//! `remote/<sha>` branch. Neither the current branch, the index nor the stash
//! changes. `.github/workflows/remote.yml` runs `cargo <args>` on it; this
//! waits for the run, prints the command's output and exit status, downloads
//! any requested paths, and deletes the branch.
//!
//! The repository is public, so hosted runners cost nothing, and a heavy test
//! run no longer loads this machine or fills its `target/`. Pushes to
//! `remote/*` start no other workflow.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const WORKFLOW: &str = "remote.yml";
const OSES: [(&str, &str); 3] = [
    ("linux", "ubuntu-24.04"),
    ("macos", "macos-15"),
    ("windows", "windows-2025"),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    /// Runner image name in `remote.yml`.
    pub os: &'static str,
    /// Paths (relative to the checkout) to bring back besides the log.
    pub artifacts: Vec<String>,
    /// Keep the `remote/<sha>` branch after the run.
    pub keep: bool,
    /// Arguments to `cargo`.
    pub cargo: Vec<String>,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut os = OSES[0].1;
        let mut artifacts = Vec::new();
        let mut keep = false;
        let mut rest = args.iter();
        let mut cargo = Vec::new();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "--os" => {
                    let name = rest.next().ok_or("--os needs linux, macos or windows")?;
                    os = OSES
                        .iter()
                        .find(|(short, _)| short == name)
                        .map(|(_, image)| *image)
                        .ok_or_else(|| format!("unknown --os {name}: linux, macos or windows"))?;
                }
                "--artifact" => {
                    let path = rest.next().ok_or("--artifact needs a path")?;
                    if path.is_empty() || path.starts_with('/') || path.contains("..") {
                        return Err(format!(
                            "--artifact {path}: give a path inside the checkout"
                        ));
                    }
                    artifacts.push(path.clone());
                }
                "--keep" => keep = true,
                "--" => {
                    cargo.extend(rest.by_ref().cloned());
                }
                _ => {
                    cargo.push(arg.clone());
                    cargo.extend(rest.by_ref().cloned());
                }
            }
        }
        if cargo.is_empty() {
            return Err(
                "give the cargo arguments to run, e.g. `nextest run -p fission-pcode`".into(),
            );
        }
        if let Some(bad) = cargo.iter().find(|a| a.chars().any(char::is_whitespace)) {
            // The workflow splits the command on whitespace.
            return Err(format!(
                "argument {bad:?} contains whitespace, which the remote run would split"
            ));
        }
        Ok(Options {
            os,
            artifacts,
            keep,
            cargo,
        })
    }

    /// The `gh workflow run` arguments for `branch`.
    fn dispatch_args(&self, branch: &str) -> Vec<String> {
        let mut args = vec![
            "workflow".to_string(),
            "run".into(),
            WORKFLOW.into(),
            "--ref".into(),
            branch.into(),
            "-f".into(),
            format!("command={}", self.cargo.join(" ")),
            "-f".into(),
            format!("os={}", self.os),
        ];
        if !self.artifacts.is_empty() {
            args.push("-f".into());
            args.push(format!("artifacts={}", self.artifacts.join(" ")));
        }
        args
    }
}

fn output(program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(args).stderr(Stdio::inherit());
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    if !out.status.success() {
        return Err(format!("{program} {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn status(program: &str, args: &[String]) -> Result<bool, String> {
    Command::new(program)
        .args(args)
        .status()
        .map(|s| s.success())
        .map_err(|err| format!("{program}: {err}"))
}

/// A commit of the working tree as it is, parented on HEAD, written through
/// a temporary index so the real one is untouched.
fn snapshot(root: &Path) -> Result<String, String> {
    let index = std::env::temp_dir().join(format!(
        "fission-remote-index-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let index = index.to_string_lossy().to_string();
    let root = root.to_str().ok_or("checkout path is not UTF-8")?;
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let git = |args: &[&str], env: &[(&str, &str)]| {
        let mut all = vec!["-C", root];
        all.extend_from_slice(args);
        output("git", &all, env)
    };
    let result = (|| {
        git(&["read-tree", "HEAD"], &env)?;
        git(&["add", "-A", "--", "."], &env)?;
        let tree = git(&["write-tree"], &env)?;
        let head = git(&["rev-parse", "HEAD"], &[])?;
        if tree == git(&["rev-parse", "HEAD^{tree}"], &[])? {
            return Ok(head);
        }
        git(
            &[
                "commit-tree",
                &tree,
                "-p",
                &head,
                "-m",
                "cargo xtask remote: working tree",
            ],
            &[],
        )
    })();
    let _ = std::fs::remove_file(&index);
    result
}

fn find_run(branch: &str, commit: &str) -> Result<String, String> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(90) {
        let id = output(
            "gh",
            &[
                "run",
                "list",
                "--workflow",
                WORKFLOW,
                "--branch",
                branch,
                "--limit",
                "5",
                "--json",
                "databaseId,headSha",
                "--jq",
                &format!(".[] | select(.headSha == \"{commit}\") | .databaseId"),
            ],
            &[],
        )?;
        if let Some(id) = id.lines().next() {
            return Ok(id.to_string());
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    Err(format!("no {WORKFLOW} run appeared for {branch}"))
}

pub fn run(options: &Options) -> ExitCode {
    match run_inner(options) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(options: &Options) -> Result<ExitCode, String> {
    let root = PathBuf::from(output("git", &["rev-parse", "--show-toplevel"], &[])?);
    let commit = snapshot(&root)?;
    let branch = format!("remote/{}", &commit[..12]);
    eprintln!(
        "remote: {} on {} (commit {})",
        options.cargo.join(" "),
        options.os,
        &commit[..12]
    );
    output(
        "git",
        &[
            "push",
            "--quiet",
            "origin",
            &format!("{commit}:refs/heads/{branch}"),
        ],
        &[],
    )?;
    let outcome = (|| {
        if !status("gh", &options.dispatch_args(&branch))? {
            return Err("could not dispatch the workflow".to_string());
        }
        let id = find_run(&branch, &commit)?;
        eprintln!("remote: run {id}");
        // Progress on stderr; the result comes from the downloaded log.
        let watch = ["run", "watch", &id, "--interval", "15", "--exit-status"].map(String::from);
        let _ = status("gh", &watch)?;
        let dir = root.join("target").join("remote").join(&id);
        let download = [
            "run",
            "download",
            &id,
            "--name",
            "remote-output",
            "--dir",
            dir.to_str().unwrap_or("target/remote"),
        ]
        .map(String::from);
        if !status("gh", &download)? {
            return Err(format!(
                "run {id} left no output; see `gh run view {id} --log`"
            ));
        }
        let log = std::fs::read_to_string(dir.join("remote.log")).unwrap_or_default();
        print!("{log}");
        let code: u8 = std::fs::read_to_string(dir.join("status"))
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .map(|s| s.clamp(0, 255) as u8)
            .unwrap_or(1);
        if !options.artifacts.is_empty() {
            eprintln!("remote: files in {}", dir.join("files").display());
        }
        Ok(ExitCode::from(code))
    })();
    if !options.keep {
        let _ = output(
            "git",
            &["push", "--quiet", "origin", "--delete", &branch],
            &[],
        );
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn options_before_the_command_and_everything_after_it_go_to_cargo() {
        let options = parse(&[
            "--os",
            "macos",
            "--artifact",
            "target/x.json",
            "nextest",
            "run",
            "--keep",
        ])
        .unwrap();
        assert_eq!(options.os, "macos-15");
        assert_eq!(options.artifacts, ["target/x.json"]);
        // `--keep` after the command is cargo's, not ours.
        assert!(!options.keep);
        assert_eq!(options.cargo, ["nextest", "run", "--keep"]);
        let options = parse(&["--keep", "--", "--version"]).unwrap();
        assert!(options.keep);
        assert_eq!(options.cargo, ["--version"]);
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["--os", "solaris", "check"]).is_err());
        assert!(parse(&["--artifact", "/etc/passwd", "check"]).is_err());
        assert!(parse(&["--artifact", "../x", "check"]).is_err());
        assert!(
            parse(&["test", "a b"]).is_err(),
            "the workflow would split it"
        );
    }

    #[test]
    fn the_snapshot_holds_the_working_tree_and_leaves_the_checkout_alone() {
        let dir = std::env::temp_dir().join(format!("fission-remote-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("tracked"), "one").unwrap();
        std::fs::write(dir.join(".gitignore"), "ignored\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "base"]);
        let head = git(&["rev-parse", "HEAD"]);

        // A clean tree is HEAD itself.
        assert_eq!(snapshot(&dir).unwrap(), head);

        std::fs::write(dir.join("tracked"), "two").unwrap();
        std::fs::write(dir.join("untracked"), "new").unwrap();
        std::fs::write(dir.join("ignored"), "secret").unwrap();
        let status_before = git(&["status", "--porcelain"]);
        let commit = snapshot(&dir).unwrap();

        assert_ne!(commit, head);
        assert_eq!(git(&["rev-parse", &format!("{commit}^")]), head);
        assert_eq!(git(&["show", &format!("{commit}:tracked")]), "two");
        assert_eq!(git(&["show", &format!("{commit}:untracked")]), "new");
        assert!(
            git(&["ls-tree", "--name-only", &commit])
                .lines()
                .all(|f| f != "ignored")
        );
        // Branch, index and worktree as they were.
        assert_eq!(git(&["rev-parse", "HEAD"]), head);
        assert_eq!(git(&["status", "--porcelain"]), status_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dispatch_names_the_branch_command_and_runner() {
        let options = parse(&[
            "--artifact",
            "a",
            "--artifact",
            "b",
            "nextest",
            "run",
            "-p",
            "fission-pcode",
        ])
        .unwrap();
        assert_eq!(
            options.dispatch_args("remote/0123456789ab"),
            [
                "workflow",
                "run",
                "remote.yml",
                "--ref",
                "remote/0123456789ab",
                "-f",
                "command=nextest run -p fission-pcode",
                "-f",
                "os=ubuntu-24.04",
                "-f",
                "artifacts=a b",
            ]
        );
    }
}
