//! `cargo xtask cleanup`: find, and with `--apply` remove, stale build output
//! and finished worktrees of this repository.
//!
//! Two kinds of candidate, each decided from facts rather than names:
//!
//! - **Build directories**: `target` and `target-*` directly inside the main
//!   checkout, inside any registered worktree, and inside any directory under
//!   `<main>/.claude/worktrees/` (registered or not). A directory a running
//!   process uses (cwd, open file, or a path on its command line) is kept, as
//!   is one written within `--min-age-hours`. The main checkout's `target`
//!   is kept unless `--include-main-target` is passed.
//! - **Worktrees** from `git worktree list --porcelain`, other than the main
//!   checkout: removable when the branch's newest pull request is merged or
//!   closed, or when HEAD is already contained in the base ref, and only if
//!   the working tree is clean, every commit is on some remote ref, no process
//!   uses it, and git last touched it more than `--min-age-hours` ago.
//!
//! Report mode (the default) changes nothing. `--apply` prints the removal
//! list first, deletes build directories (never through a symlink, never
//! outside a worktree root), removes worktrees with `git worktree remove`
//! (no `--force`) and finishes with `git worktree prune`.
//!
//! Directories under `--hints` (default `~/fission-benchmark`) are sized and
//! listed as hints only; nothing there is ever a candidate.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Process names whose open files are scanned, as `lsof -c` prefixes.
const BUILD_PROCESSES: [&str; 8] = [
    "cargo",
    "rustc",
    "clippy-dr",
    "rust-anal",
    "rustdoc",
    "sccache",
    "fission",
    "xtask",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub apply: bool,
    pub include_main_target: bool,
    pub use_gh: bool,
    pub base: String,
    pub min_age_hours: u64,
    pub hint_dir: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut options = Options {
            apply: false,
            include_main_target: false,
            use_gh: true,
            base: "origin/main".to_string(),
            min_age_hours: 24,
            hint_dir: std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("fission-benchmark")),
        };
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--apply" => options.apply = true,
                "--include-main-target" => options.include_main_target = true,
                "--no-gh" => options.use_gh = false,
                "--no-hints" => options.hint_dir = None,
                "--base" => {
                    options.base = iter.next().ok_or("--base needs a git ref")?.to_string();
                }
                "--min-age-hours" => {
                    let value = iter.next().ok_or("--min-age-hours needs a number")?;
                    options.min_age_hours = value
                        .parse()
                        .map_err(|_| format!("--min-age-hours: `{value}` is not a whole number"))?;
                }
                "--hints" => {
                    let dir = iter.next().ok_or("--hints needs a directory")?;
                    options.hint_dir = Some(PathBuf::from(dir));
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(options)
    }
}

// ---------------------------------------------------------------------------
// Processes

/// A running process that touches `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub pid: u32,
    pub command: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct Processes {
    pub uses: Vec<Use>,
}

impl Processes {
    /// The first process whose cwd, open file or argument lies under `dir`.
    pub fn user_of(&self, dir: &Path) -> Option<&Use> {
        self.uses.iter().find(|u| u.path.starts_with(dir))
    }
}

/// Parse `lsof -F pcn` output into one `Use` per absolute name.
pub fn parse_lsof(text: &str) -> Vec<Use> {
    let mut uses = Vec::new();
    let mut pid = 0u32;
    let mut command = String::new();
    for line in text.lines() {
        let Some(tag) = line.chars().next() else {
            continue;
        };
        let value = &line[tag.len_utf8()..];
        match tag {
            'p' => {
                pid = value.parse().unwrap_or(0);
                command.clear();
            }
            'c' => command = value.to_string(),
            'n' if value.starts_with('/') => {
                // lsof annotates some names, e.g. "/path (deleted)".
                let path = value.split(" (").next().unwrap_or(value);
                uses.push(Use {
                    pid,
                    command: command.clone(),
                    path: PathBuf::from(path),
                });
            }
            _ => {}
        }
    }
    uses
}

/// Parse `ps -axww -o pid=,command=` output: every argument that names a
/// path (absolute, or relative to the process's cwd when known) is a use.
pub fn parse_ps(text: &str, cwd_of: &BTreeMap<u32, PathBuf>) -> Vec<Use> {
    let mut uses = Vec::new();
    for line in text.lines() {
        let Some((pid, command)) = line.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        let name = command
            .split_whitespace()
            .next()
            .and_then(|exe| exe.rsplit('/').next())
            .unwrap_or_default()
            .to_string();
        for token in command.split_whitespace() {
            let token = token.rsplit_once('=').map_or(token, |(_, value)| value);
            if !token.contains('/') {
                continue;
            }
            let path = if token.starts_with('/') {
                PathBuf::from(token)
            } else if let Some(cwd) = cwd_of.get(&pid) {
                cwd.join(token)
            } else {
                continue;
            };
            uses.push(Use {
                pid,
                command: name.clone(),
                path: lexical(&path),
            });
        }
    }
    uses
}

/// Resolve `.` and `..` without touching the filesystem.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Run a scanner whose non-zero exit can just mean "no match" (lsof).
fn scan_output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every process's cwd, the open files of build processes, the paths on
/// every command line, and this process's own executable.
pub fn scan_processes() -> Result<Processes, String> {
    let cwd_text = scan_output("lsof", &["-n", "-P", "-w", "-F", "pcn", "-d", "cwd"])?;
    let cwds = parse_lsof(&cwd_text);
    if cwds.is_empty() {
        return Err("`lsof -d cwd` listed no process; cannot tell what is in use".into());
    }
    let cwd_of: BTreeMap<u32, PathBuf> = cwds.iter().map(|u| (u.pid, u.path.clone())).collect();
    let mut args = vec!["-n", "-P", "-w", "-F", "pcn"];
    for name in BUILD_PROCESSES {
        args.extend(["-c", name]);
    }
    let open_files = parse_lsof(&scan_output("lsof", &args)?);
    let ps_text = scan_output("ps", &["-axww", "-o", "pid=,command="])?;
    if ps_text.trim().is_empty() {
        return Err("`ps` listed no process; cannot tell what is in use".into());
    }
    let mut uses = cwds;
    uses.extend(open_files);
    uses.extend(parse_ps(&ps_text, &cwd_of));
    if let Ok(exe) = std::env::current_exe().and_then(|p| p.canonicalize()) {
        uses.push(Use {
            pid: std::process::id(),
            command: "xtask".into(),
            path: exe,
        });
    }
    Ok(Processes { uses })
}

// ---------------------------------------------------------------------------
// Pull requests

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pr {
    pub number: u64,
    pub state: PrState,
}

/// Parse `number<TAB>state<TAB>branch` lines; the newest PR per branch wins.
pub fn parse_prs(text: &str) -> BTreeMap<String, Pr> {
    let mut prs: BTreeMap<String, Pr> = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(number), Some(state), Some(branch)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(number) = number.parse() else {
            continue;
        };
        let state = match state {
            "OPEN" => PrState::Open,
            "MERGED" => PrState::Merged,
            "CLOSED" => PrState::Closed,
            _ => continue,
        };
        let pr = Pr { number, state };
        prs.entry(branch.to_string())
            .and_modify(|old| {
                if pr.number > old.number {
                    *old = pr;
                }
            })
            .or_insert(pr);
    }
    prs
}

fn fetch_prs(main: &Path) -> Result<BTreeMap<String, Pr>, String> {
    let out = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "5000",
            "--json",
            "number,state,headRefName,isCrossRepository",
            "--jq",
            r#".[] | select(.isCrossRepository | not) | "\(.number)\t\(.state)\t\(.headRefName)""#,
        ])
        .current_dir(main)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !out.status.success() {
        return Err("`gh pr list` failed".into());
    }
    Ok(parse_prs(&String::from_utf8_lossy(&out.stdout)))
}

// ---------------------------------------------------------------------------
// Worktrees

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            list.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(path),
                ..Worktree::default()
            });
            continue;
        }
        let Some(wt) = current.as_mut() else {
            continue;
        };
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "HEAD" => wt.head = Some(value.to_string()),
            "branch" => {
                wt.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_string(),
                );
            }
            "bare" => wt.bare = true,
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            _ => {}
        }
    }
    list.extend(current);
    list
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed in {}",
            args.join(" "),
            dir.display()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Registered worktrees; the first entry is the main checkout.
pub fn list_worktrees(any_checkout: &Path) -> Result<Vec<Worktree>, String> {
    let list = parse_worktrees(&git(any_checkout, &["worktree", "list", "--porcelain"])?);
    if list.is_empty() {
        return Err("`git worktree list` returned nothing".into());
    }
    Ok(list)
}

// ---------------------------------------------------------------------------
// Survey

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Remove(String),
    Keep(String),
}

impl Verdict {
    pub fn removes(&self) -> bool {
        matches!(self, Verdict::Remove(_))
    }
    fn reason(&self) -> &str {
        match self {
            Verdict::Remove(r) | Verdict::Keep(r) => r,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub label: String,
    pub bytes: Option<u64>,
    pub newest: Option<SystemTime>,
    pub verdict: Verdict,
}

pub struct Env {
    pub processes: Processes,
    /// `None` when the PR lookup was skipped or failed.
    pub prs: Option<BTreeMap<String, Pr>>,
    pub now: SystemTime,
}

pub struct Survey {
    pub main: PathBuf,
    /// Directories a build directory may sit directly inside.
    pub roots: Vec<PathBuf>,
    pub build_dirs: Vec<Item>,
    pub worktrees: Vec<Item>,
    pub prunable: usize,
    pub hints: Vec<(PathBuf, u64)>,
    pub notes: Vec<String>,
}

impl Survey {
    pub fn removals(&self) -> impl Iterator<Item = &Item> {
        self.build_dirs
            .iter()
            .chain(&self.worktrees)
            .filter(|i| i.verdict.removes())
    }
}

pub fn is_build_dir_name(name: &str) -> bool {
    name == "target" || name.starts_with("target-")
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn hours(age: Duration) -> String {
    let h = age.as_secs() / 3600;
    if h >= 48 {
        format!("{}d", h / 24)
    } else {
        format!("{h}h")
    }
}

/// Newest modification time among git's per-worktree files.
fn git_activity(checkout: &Path) -> Option<SystemTime> {
    let gitdir = git(checkout, &["rev-parse", "--absolute-git-dir"]).ok()?;
    let gitdir = PathBuf::from(gitdir.trim());
    ["HEAD", "index", "logs/HEAD"]
        .iter()
        .filter_map(|f| std::fs::metadata(gitdir.join(f)).ok()?.modified().ok())
        .max()
}

pub fn survey(any_checkout: &Path, options: &Options, env: &Env) -> Result<Survey, String> {
    let list = list_worktrees(any_checkout)?;
    let main = canonical(&list[0].path);
    if !git_ok(&main, &["rev-parse", "--verify", "--quiet", &options.base]) {
        return Err(format!("base ref `{}` does not exist", options.base));
    }
    let min_age = Duration::from_secs(options.min_age_hours * 3600);
    // A time after `now` was written while this survey ran: recent.
    let recent = |t: Option<SystemTime>| {
        t.is_some_and(|t| env.now.duration_since(t).map_or(true, |age| age < min_age))
    };
    let mut notes = Vec::new();
    if env.prs.is_none() {
        notes.push(format!(
            "no PR lookup: a worktree is merged only when its HEAD is in {}",
            options.base
        ));
    }

    // Roots: every live worktree, plus unregistered dirs under .claude/worktrees.
    let live: Vec<&Worktree> = list.iter().filter(|w| !w.bare && !w.prunable).collect();
    let mut roots: Vec<PathBuf> = live.iter().map(|w| canonical(&w.path)).collect();
    if let Ok(entries) = std::fs::read_dir(main.join(".claude/worktrees")) {
        let mut orphans: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| canonical(&e.path()))
            .filter(|p| !roots.contains(p))
            .collect();
        orphans.sort();
        roots.extend(orphans);
    }

    // Build directories.
    let mut build_dirs = Vec::new();
    for root in &roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut names: Vec<_> = entries
            .flatten()
            .filter(|e| is_build_dir_name(&e.file_name().to_string_lossy()))
            .collect();
        names.sort_by_key(|e| e.file_name());
        for entry in names {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = root.join(entry.file_name());
            let verdict = if kind.is_symlink() {
                Verdict::Keep("symlink; never followed".into())
            } else if !kind.is_dir() {
                continue;
            } else if *root == main && entry.file_name() == "target" && !options.include_main_target
            {
                Verdict::Keep("main checkout's target (--include-main-target)".into())
            } else if let Some(u) = env.processes.user_of(&path) {
                Verdict::Keep(format!("in use by pid {} ({})", u.pid, u.command))
            } else {
                Verdict::Remove("no running process uses it".into())
            };
            build_dirs.push(Item {
                path,
                label: String::new(),
                bytes: None,
                newest: None,
                verdict,
            });
        }
    }
    let sized: Vec<usize> = (0..build_dirs.len())
        .filter(|&i| !build_dirs[i].verdict.reason().starts_with("symlink"))
        .collect();
    let jobs: Vec<(PathBuf, bool)> = sized
        .iter()
        .map(|&i| (build_dirs[i].path.clone(), false))
        .collect();
    for (&i, (bytes, newest)) in sized.iter().zip(walk_all(&jobs)) {
        let item = &mut build_dirs[i];
        item.bytes = Some(bytes);
        item.newest = newest;
        if item.verdict.removes() && recent(newest) {
            item.verdict = Verdict::Keep(format!(
                "written {} ago (--min-age-hours {})",
                hours(env.now.duration_since(newest.unwrap()).unwrap_or_default()),
                options.min_age_hours
            ));
        }
    }

    // Worktrees.
    let mut worktrees = Vec::new();
    for wt in live.iter().filter(|w| canonical(&w.path) != main) {
        let path = canonical(&wt.path);
        let label = match (&wt.branch, &wt.head) {
            (Some(b), _) => b.clone(),
            (None, Some(h)) => format!("detached {}", &h[..h.len().min(9)]),
            (None, None) => "?".into(),
        };
        let activity = git_activity(&path);
        let verdict = worktree_verdict(wt, &path, activity, options, env, &recent, &build_dirs);
        worktrees.push(Item {
            path,
            label,
            bytes: None,
            newest: activity,
            verdict,
        });
    }
    let removable: Vec<usize> = (0..worktrees.len())
        .filter(|&i| worktrees[i].verdict.removes())
        .collect();
    let jobs: Vec<(PathBuf, bool)> = removable
        .iter()
        .map(|&i| (worktrees[i].path.clone(), true))
        .collect();
    for (&i, (bytes, _)) in removable.iter().zip(walk_all(&jobs)) {
        worktrees[i].bytes = Some(bytes);
    }

    // Hints.
    let mut hints = Vec::new();
    if let Some(dir) = &options.hint_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            let jobs: Vec<(PathBuf, bool)> = paths.iter().map(|p| (p.clone(), false)).collect();
            hints = paths
                .into_iter()
                .zip(walk_all(&jobs))
                .map(|(p, (bytes, _))| (p, bytes))
                .collect();
            hints.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        }
    }

    Ok(Survey {
        main,
        roots,
        build_dirs,
        worktrees,
        prunable: list.iter().filter(|w| w.prunable).count(),
        hints,
        notes,
    })
}

fn worktree_verdict(
    wt: &Worktree,
    path: &Path,
    activity: Option<SystemTime>,
    options: &Options,
    env: &Env,
    recent: &dyn Fn(Option<SystemTime>) -> bool,
    build_dirs: &[Item],
) -> Verdict {
    if wt.locked {
        return Verdict::Keep("locked".into());
    }
    if let Some(u) = env.processes.user_of(path) {
        return Verdict::Keep(format!("in use by pid {} ({})", u.pid, u.command));
    }
    let Some(head) = wt.head.as_deref() else {
        return Verdict::Keep("no HEAD".into());
    };
    // --no-optional-locks: do not refresh the index, so a report run does not
    // make the worktree look recently used to the next run.
    match git(
        path,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain",
            "--untracked-files=normal",
        ],
    ) {
        Ok(status) if !status.trim().is_empty() => {
            return Verdict::Keep("uncommitted or untracked changes".into());
        }
        Ok(_) => {}
        Err(_) => return Verdict::Keep("git status failed".into()),
    }
    match git(path, &["rev-list", "--count", head, "--not", "--remotes"])
        .map(|n| n.trim().parse::<u64>())
    {
        Ok(Ok(0)) => {}
        Ok(Ok(n)) => return Verdict::Keep(format!("{n} commit(s) on no remote ref")),
        _ => return Verdict::Keep("cannot count unpushed commits".into()),
    }
    let pr = wt
        .branch
        .as_ref()
        .and_then(|b| env.prs.as_ref()?.get(b).copied());
    let verdict = match pr {
        Some(Pr {
            number,
            state: PrState::Open,
        }) => return Verdict::Keep(format!("PR #{number} open")),
        Some(Pr {
            number,
            state: PrState::Merged,
        }) => Verdict::Remove(format!("PR #{number} merged")),
        Some(Pr {
            number,
            state: PrState::Closed,
        }) => Verdict::Remove(format!("PR #{number} closed")),
        None if git_ok(path, &["merge-base", "--is-ancestor", head, &options.base]) => {
            Verdict::Remove(format!("HEAD is in {}", options.base))
        }
        None => return Verdict::Keep(format!("not in {} and no closed PR", options.base)),
    };
    if recent(activity) {
        let age = activity
            .and_then(|t| env.now.duration_since(t).ok())
            .unwrap_or_default();
        return Verdict::Keep(format!(
            "git activity {} ago (--min-age-hours {})",
            hours(age),
            options.min_age_hours
        ));
    }
    // `git worktree remove` deletes ignored files too; a build dir we keep
    // must keep its worktree.
    if let Some(kept) = build_dirs.iter().find(|b| {
        b.path.parent() == Some(path)
            && !b.verdict.removes()
            && !b.verdict.reason().starts_with("symlink")
    }) {
        return Verdict::Keep(format!(
            "its {} is kept: {}",
            kept.path.file_name().unwrap_or_default().to_string_lossy(),
            kept.verdict.reason()
        ));
    }
    verdict
}

// ---------------------------------------------------------------------------
// Sizing

#[cfg(unix)]
fn disk_bytes(meta: &std::fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // Cargo hard-links binaries into target/<profile>/; count each inode once.
    if meta.nlink() > 1 && !meta.is_dir() && !seen.insert((meta.dev(), meta.ino())) {
        return 0;
    }
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn disk_bytes(meta: &std::fs::Metadata, _seen: &mut HashSet<(u64, u64)>) -> u64 {
    meta.len()
}

/// Disk usage and newest mtime of a tree, never following symlinks. With
/// `skip_build_dirs`, top-level `target`/`target-*` entries are left out
/// (they are reported on their own).
pub fn walk(root: &Path, skip_build_dirs: bool) -> (u64, Option<SystemTime>) {
    let mut seen = HashSet::new();
    let mut bytes = 0;
    let mut newest: Option<SystemTime> = None;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((path, depth)) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        bytes += disk_bytes(&meta, &mut seen);
        if !meta.is_dir() {
            if let Ok(m) = meta.modified() {
                newest = newest.max(Some(m));
            }
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            if skip_build_dirs
                && depth == 0
                && is_build_dir_name(&entry.file_name().to_string_lossy())
            {
                continue;
            }
            stack.push((entry.path(), depth + 1));
        }
    }
    (bytes, newest)
}

fn walk_all(jobs: &[(PathBuf, bool)]) -> Vec<(u64, Option<SystemTime>)> {
    let results = Mutex::new(vec![(0, None); jobs.len()]);
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, skip)) = jobs.get(i) else {
                        break;
                    };
                    let result = walk(path, *skip);
                    results.lock().unwrap()[i] = result;
                }
            });
        }
    });
    results.into_inner().unwrap()
}

// ---------------------------------------------------------------------------
// Removal

/// Delete a build directory. Refuses a symlink, anything not named
/// `target`/`target-*`, and anything not sitting directly inside one of
/// `roots` once every symlink in its path is resolved.
pub fn remove_build_dir(path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a symlink; refusing", path.display()));
    }
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let resolved = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let name = resolved
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !is_build_dir_name(&name) {
        return Err(format!(
            "{} is not named target or target-*; refusing",
            resolved.display()
        ));
    }
    let inside_root = resolved
        .parent()
        .is_some_and(|parent| roots.iter().any(|r| canonical(r) == parent));
    if !inside_root {
        return Err(format!(
            "{} is not directly inside a worktree root; refusing",
            resolved.display()
        ));
    }
    // remove_dir_all unlinks symlinks inside the tree without following them.
    std::fs::remove_dir_all(&resolved).map_err(|e| format!("{}: {e}", resolved.display()))
}

/// Remove every candidate. Build directories go first, so a worktree is
/// never removed while its build output is still being deleted.
pub fn apply(survey: &Survey) -> bool {
    let removals: Vec<&Item> = survey.removals().collect();
    println!("\nRemoving {} item(s):", removals.len());
    for item in &removals {
        println!("  {}", item.path.display());
    }
    let mut ok = true;
    for item in survey.build_dirs.iter().filter(|i| i.verdict.removes()) {
        match remove_build_dir(&item.path, &survey.roots) {
            Ok(()) => println!("removed   {}", item.path.display()),
            Err(e) => {
                ok = false;
                eprintln!("FAILED    {e}");
            }
        }
    }
    for item in survey.worktrees.iter().filter(|i| i.verdict.removes()) {
        let path = item.path.to_string_lossy();
        match git(&survey.main, &["worktree", "remove", &path]) {
            Ok(_) => println!("removed   {path} (git worktree remove)"),
            Err(e) => {
                ok = false;
                eprintln!("FAILED    {e}");
            }
        }
    }
    match git(&survey.main, &["worktree", "prune"]) {
        Ok(_) => println!("pruned    stale worktree entries (git worktree prune)"),
        Err(e) => {
            ok = false;
            eprintln!("FAILED    {e}");
        }
    }
    ok
}

// ---------------------------------------------------------------------------
// Report

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn row(out: &mut String, item: &Item, now: SystemTime) {
    let action = if item.verdict.removes() {
        "REMOVE"
    } else {
        "keep"
    };
    let size = item.bytes.map(gb).unwrap_or_else(|| "-".into());
    let age = item
        .newest
        .map(|t| now.duration_since(t).unwrap_or_default())
        .map(|a| format!("{} ago", hours(a)))
        .unwrap_or_else(|| "-".into());
    let label = if item.label.is_empty() {
        String::new()
    } else {
        format!(" [{}]", item.label)
    };
    let _ = writeln!(
        out,
        "  {action:<6} {size:>9} {age:>8}  {}{label}: {}",
        item.path.display(),
        item.verdict.reason()
    );
}

fn total(items: &[Item]) -> (usize, u64) {
    items
        .iter()
        .filter(|i| i.verdict.removes())
        .fold((0, 0), |(n, b), i| (n + 1, b + i.bytes.unwrap_or(0)))
}

pub fn report(survey: &Survey, now: SystemTime) -> String {
    let mut out = String::new();
    let (nb, bb) = total(&survey.build_dirs);
    let (nw, bw) = total(&survey.worktrees);
    let _ = writeln!(
        out,
        "Build directories: {} found, {nb} removable ({})",
        survey.build_dirs.len(),
        gb(bb)
    );
    for item in &survey.build_dirs {
        row(&mut out, item, now);
    }
    let _ = writeln!(
        out,
        "\nWorktrees: {} besides the main checkout, {nw} removable ({}, build dirs excluded; age = last git activity)",
        survey.worktrees.len(),
        gb(bw)
    );
    let mut worktrees: Vec<&Item> = survey.worktrees.iter().collect();
    worktrees.sort_by_key(|i| (!i.verdict.removes(), i.path.clone()));
    for item in worktrees {
        row(&mut out, item, now);
    }
    if survey.prunable > 0 {
        let _ = writeln!(
            out,
            "  {} entr(y/ies) whose directory is gone; `git worktree prune` drops them",
            survey.prunable
        );
    }
    if !survey.hints.is_empty() {
        let hint_total: u64 = survey.hints.iter().map(|h| h.1).sum();
        let _ = writeln!(
            out,
            "\nElsewhere (hints only; never removed): {}",
            gb(hint_total)
        );
        for (path, bytes) in survey.hints.iter().take(15) {
            let _ = writeln!(out, "  {:>9}  {}", gb(*bytes), path.display());
        }
    }
    for note in &survey.notes {
        let _ = writeln!(out, "\nnote: {note}");
    }
    let _ = writeln!(
        out,
        "\nSummary: {nb} build dir(s) ({}) + {nw} worktree(s) ({}) removable = {}",
        gb(bb),
        gb(bw),
        gb(bb + bw)
    );
    out
}

// ---------------------------------------------------------------------------
// Entry point

pub fn run(options: &Options) -> ExitCode {
    match run_inner(options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("cleanup: {message}");
            ExitCode::from(2)
        }
    }
}

fn run_inner(options: &Options) -> Result<bool, String> {
    let here = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let list = list_worktrees(&here)?;
    let main = canonical(&list[0].path);
    let processes = scan_processes()?;
    let mut prs = None;
    let mut gh_note = None;
    if options.use_gh {
        match fetch_prs(&main) {
            Ok(map) => prs = Some(map),
            Err(e) => gh_note = Some(format!("PR lookup skipped: {e}")),
        }
    }
    let env = Env {
        processes,
        prs,
        now: SystemTime::now(),
    };
    let mut survey = survey(&main, options, &env)?;
    survey.notes.extend(gh_note);
    print!("{}", report(&survey, env.now));
    if !options.apply {
        if survey.removals().next().is_some() {
            println!("Report only; nothing was changed. Re-run with --apply to remove.");
        }
        return Ok(true);
    }
    Ok(apply(&survey))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A temporary directory removed on drop.
    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Temp {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("xtask-cleanup-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Temp(dir.canonicalize().unwrap())
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn commit(dir: &Path, file: &str) {
        std::fs::write(dir.join(file), file).unwrap();
        sh(dir, &["add", file]);
        sh(dir, &["commit", "-q", "-m", file]);
    }

    fn options() -> Options {
        Options {
            apply: false,
            include_main_target: false,
            use_gh: false,
            base: "origin/main".into(),
            min_age_hours: 0,
            hint_dir: None,
        }
    }

    fn env(processes: Vec<Use>, prs: &[(&str, u64, PrState)]) -> Env {
        Env {
            processes: Processes { uses: processes },
            prs: Some(
                prs.iter()
                    .map(|(b, number, state)| {
                        (
                            b.to_string(),
                            Pr {
                                number: *number,
                                state: *state,
                            },
                        )
                    })
                    .collect(),
            ),
            now: SystemTime::now(),
        }
    }

    /// A clone of a bare origin with one worktree per case:
    ///
    /// | worktree    | state                                          | expected |
    /// |-------------|------------------------------------------------|----------|
    /// | merged      | own commit merged into origin/main, clean      | remove   |
    /// | closed      | pushed, unmerged, PR closed                    | remove   |
    /// | unmerged    | pushed, unmerged, no PR                        | keep     |
    /// | open        | in origin/main by ancestry, PR open            | keep     |
    /// | dirty       | in origin/main, untracked file                 | keep     |
    /// | unpushed    | PR merged, but a local commit on no remote ref | keep     |
    /// | busy        | in origin/main, a process cwd inside it        | keep     |
    /// | busytarget  | in origin/main, its target/ open in a process  | keep     |
    /// | locked      | in origin/main, `git worktree lock`            | keep     |
    struct Fixture {
        _tmp: Temp,
        main: PathBuf,
        wts: PathBuf,
    }

    impl Fixture {
        fn new() -> Fixture {
            let tmp = Temp::new();
            let origin = tmp.0.join("origin.git");
            sh(
                &tmp.0,
                &["init", "-q", "--bare", "-b", "main", "origin.git"],
            );
            sh(&tmp.0, &["clone", "-q", &origin.to_string_lossy(), "repo"]);
            let main = tmp.0.join("repo");
            std::fs::write(main.join(".gitignore"), "target/\ntarget-*/\n").unwrap();
            sh(&main, &["add", ".gitignore"]);
            commit(&main, "README");
            sh(&main, &["push", "-q", "origin", "main"]);
            let wts = tmp.0.join("wts");
            let add = |name: &str| {
                let path = wts.join(name);
                sh(
                    &main,
                    &[
                        "worktree",
                        "add",
                        "-q",
                        "-b",
                        name,
                        &path.to_string_lossy(),
                        "main",
                    ],
                );
                path
            };

            let merged = add("merged");
            commit(&merged, "merged.txt");
            sh(&merged, &["push", "-q", "origin", "merged"]);
            sh(&main, &["merge", "-q", "--no-ff", "-m", "merge", "merged"]);
            sh(&main, &["push", "-q", "origin", "main"]);

            for name in ["closed", "unmerged"] {
                let wt = add(name);
                commit(&wt, &format!("{name}.txt"));
                sh(&wt, &["push", "-q", "origin", name]);
            }
            for name in ["open", "dirty", "busy", "busytarget", "locked"] {
                add(name);
            }
            std::fs::write(wts.join("dirty/scratch.txt"), "x").unwrap();
            let unpushed = add("unpushed");
            commit(&unpushed, "unpushed.txt");
            sh(
                &main,
                &["worktree", "lock", &wts.join("locked").to_string_lossy()],
            );
            sh(&main, &["fetch", "-q", "origin"]);

            for dir in ["merged/target", "busytarget/target", "unmerged/target"] {
                std::fs::create_dir_all(wts.join(dir).join("debug")).unwrap();
                std::fs::write(wts.join(dir).join("debug/bin"), "x").unwrap();
            }
            Fixture {
                _tmp: tmp,
                main: main.canonicalize().unwrap(),
                wts: wts.canonicalize().unwrap(),
            }
        }

        fn processes(&self) -> Vec<Use> {
            vec![
                Use {
                    pid: 11,
                    command: "zsh".into(),
                    path: self.wts.join("busy"),
                },
                Use {
                    pid: 12,
                    command: "rustc".into(),
                    path: self.wts.join("busytarget/target/debug/bin"),
                },
            ]
        }

        fn prs() -> Vec<(&'static str, u64, PrState)> {
            vec![
                ("closed", 2, PrState::Closed),
                ("open", 3, PrState::Open),
                ("unpushed", 4, PrState::Merged),
            ]
        }
    }

    fn verdicts(items: &[Item]) -> BTreeMap<String, bool> {
        items
            .iter()
            .map(|i| {
                let name = i.path.file_name().unwrap().to_string_lossy().into_owned();
                let parent = i
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (format!("{parent}/{name}"), i.verdict.removes())
            })
            .collect()
    }

    #[test]
    fn only_finished_clean_pushed_unused_worktrees_are_selected() {
        let fx = Fixture::new();
        let env = env(fx.processes(), &Fixture::prs());
        let survey = survey(&fx.main, &options(), &env).unwrap();
        let got = verdicts(&survey.worktrees);
        let want: BTreeMap<String, bool> = [
            ("wts/merged", true),
            ("wts/closed", true),
            ("wts/unmerged", false),
            ("wts/open", false),
            ("wts/dirty", false),
            ("wts/unpushed", false),
            ("wts/busy", false),
            ("wts/busytarget", false),
            ("wts/locked", false),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
        assert_eq!(got, want, "{}", report(&survey, env.now));
        assert!(
            survey.worktrees.iter().all(|w| w.path != fx.main),
            "the main checkout is never a worktree candidate"
        );
    }

    #[test]
    fn without_a_pr_lookup_only_ancestry_counts_as_merged() {
        let fx = Fixture::new();
        let mut env = env(fx.processes(), &[]);
        env.prs = None;
        let survey = survey(&fx.main, &options(), &env).unwrap();
        let got = verdicts(&survey.worktrees);
        assert!(got["wts/merged"]);
        assert!(!got["wts/closed"], "a closed PR is unknown without gh");
        // No PR says "open", so this branch, already in origin/main, goes.
        assert!(got["wts/open"]);
        assert!(!got["wts/dirty"] && !got["wts/unpushed"] && !got["wts/busy"]);
    }

    #[test]
    fn recent_git_activity_keeps_every_worktree() {
        let fx = Fixture::new();
        let env = env(fx.processes(), &Fixture::prs());
        let options = Options {
            min_age_hours: 24,
            ..options()
        };
        let survey = survey(&fx.main, &options, &env).unwrap();
        assert!(survey.worktrees.iter().all(|w| !w.verdict.removes()));
        assert!(survey.build_dirs.iter().all(|b| !b.verdict.removes()));
    }

    #[test]
    fn build_dirs_skip_main_target_symlinks_and_busy_dirs() {
        let fx = Fixture::new();
        let outside = Temp::new();
        std::fs::write(outside.0.join("keep.txt"), "x").unwrap();
        for dir in ["target/debug", "target-old/debug", "target-busy"] {
            std::fs::create_dir_all(fx.main.join(dir)).unwrap();
        }
        std::fs::write(fx.main.join("target-old/debug/bin"), "x").unwrap();
        std::os::unix::fs::symlink(&outside.0, fx.main.join("target-link")).unwrap();
        // An unregistered directory under .claude/worktrees is still a root.
        let orphan = fx.main.join(".claude/worktrees/orphan/target");
        std::fs::create_dir_all(&orphan).unwrap();
        let mut processes = fx.processes();
        processes.push(Use {
            pid: 13,
            command: "cargo".into(),
            path: fx.main.join("target-busy"),
        });
        let env = env(processes, &Fixture::prs());

        let survey = survey(&fx.main, &options(), &env).unwrap();
        let got = verdicts(&survey.build_dirs);
        assert_eq!(got["repo/target"], false, "main target needs the flag");
        assert_eq!(got["repo/target-old"], true);
        assert_eq!(got["repo/target-busy"], false);
        assert_eq!(
            got["repo/target-link"], false,
            "symlinks are never candidates"
        );
        assert_eq!(got["orphan/target"], true);
        assert_eq!(got["merged/target"], true);
        assert_eq!(got["unmerged/target"], true, "unused output of live work");
        assert_eq!(got["busytarget/target"], false);

        let with_main = Options {
            include_main_target: true,
            ..options()
        };
        let survey = super::survey(&fx.main, &with_main, &env).unwrap();
        assert!(verdicts(&survey.build_dirs)["repo/target"]);
    }

    #[test]
    fn a_file_written_during_the_survey_keeps_its_build_dir() {
        let fx = Fixture::new();
        let dir = fx.main.join("target-live");
        std::fs::create_dir(&dir).unwrap();
        let file = std::fs::File::create(dir.join("obj")).unwrap();
        let env = env(Vec::new(), &[]);
        // The survey's clock was read before this file's last write.
        file.set_modified(env.now + Duration::from_secs(5)).unwrap();
        let survey = survey(&fx.main, &options(), &env).unwrap();
        assert!(!verdicts(&survey.build_dirs)["repo/target-live"]);
    }

    #[test]
    fn apply_removes_exactly_the_selected_items() {
        let fx = Fixture::new();
        let outside = Temp::new();
        std::fs::write(outside.0.join("keep.txt"), "x").unwrap();
        std::fs::create_dir_all(fx.main.join("target-old")).unwrap();
        std::fs::create_dir_all(fx.main.join("target/debug")).unwrap();
        std::os::unix::fs::symlink(&outside.0, fx.main.join("target-link")).unwrap();
        let env = env(fx.processes(), &Fixture::prs());
        let survey = survey(&fx.main, &options(), &env).unwrap();
        assert!(apply(&survey));

        for gone in ["wts/merged", "wts/closed"] {
            assert!(!fx.main.parent().unwrap().join(gone).exists(), "{gone}");
        }
        assert!(!fx.main.join("target-old").exists());
        assert!(!fx.wts.join("unmerged/target").exists());
        for kept in [
            "open",
            "dirty",
            "unpushed",
            "busy",
            "busytarget/target",
            "locked",
        ] {
            assert!(fx.wts.join(kept).exists(), "{kept}");
        }
        assert!(fx.wts.join("dirty/scratch.txt").exists());
        assert!(fx.main.join("target/debug").exists(), "main target kept");
        assert!(fx.main.join("target-link").exists());
        assert!(
            outside.0.join("keep.txt").exists(),
            "symlink target untouched"
        );
        let list = sh(&fx.main, &["worktree", "list", "--porcelain"]);
        assert!(!list.contains("/wts/merged\n") && !list.contains("/wts/closed\n"));
        assert!(list.contains("/wts/dirty\n"));
        // Branches are left alone.
        assert!(sh(&fx.main, &["branch", "--list", "merged"]).contains("merged"));
    }

    #[test]
    fn remove_build_dir_refuses_unsafe_paths() {
        let root = Temp::new();
        let outside = Temp::new();
        let roots = vec![root.0.clone()];
        std::fs::write(outside.0.join("keep.txt"), "x").unwrap();

        // A symlink named like a build dir, pointing outside.
        let link = root.0.join("target-link");
        std::os::unix::fs::symlink(&outside.0, &link).unwrap();
        assert!(
            remove_build_dir(&link, &roots)
                .unwrap_err()
                .contains("symlink")
        );
        assert!(outside.0.join("keep.txt").exists());

        // A real build dir outside every root.
        let stray = outside.0.join("target");
        std::fs::create_dir(&stray).unwrap();
        assert!(
            remove_build_dir(&stray, &roots)
                .unwrap_err()
                .contains("worktree root")
        );
        assert!(stray.exists());

        // Reached through a symlinked parent: resolves outside the root.
        std::os::unix::fs::symlink(&outside.0, root.0.join("via")).unwrap();
        let via = root.0.join("via/target");
        assert!(remove_build_dir(&via, &roots).is_err());
        assert!(stray.exists());

        // Nested below a root, or not named target*.
        let nested = root.0.join("crates/target");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(remove_build_dir(&nested, &roots).is_err());
        let src = root.0.join("src");
        std::fs::create_dir(&src).unwrap();
        assert!(
            remove_build_dir(&src, &roots)
                .unwrap_err()
                .contains("not named")
        );
        assert!(nested.exists() && src.exists());

        // The one allowed shape; an inner symlink is unlinked, not followed.
        let ok = root.0.join("target-x");
        std::fs::create_dir_all(ok.join("debug")).unwrap();
        std::os::unix::fs::symlink(&outside.0, ok.join("debug/out")).unwrap();
        remove_build_dir(&ok, &roots).unwrap();
        assert!(!ok.exists());
        assert!(outside.0.join("keep.txt").exists());
    }

    #[test]
    fn walk_counts_hard_links_once_and_skips_top_level_build_dirs() {
        let tmp = Temp::new();
        std::fs::write(tmp.0.join("a"), vec![1u8; 64 * 1024]).unwrap();
        let (one, _) = walk(&tmp.0, false);
        std::fs::hard_link(tmp.0.join("a"), tmp.0.join("b")).unwrap();
        let (linked, newest) = walk(&tmp.0, false);
        assert_eq!(one, linked);
        assert!(newest.is_some());
        std::fs::create_dir(tmp.0.join("target")).unwrap();
        std::fs::write(tmp.0.join("target/big"), vec![1u8; 256 * 1024]).unwrap();
        assert_eq!(walk(&tmp.0, true).0, linked);
        assert!(walk(&tmp.0, false).0 > linked);
    }

    #[test]
    fn real_process_scan_sees_a_cwd() {
        if Command::new("lsof").arg("-v").output().is_err() {
            eprintln!("lsof not installed; skipping");
            return;
        }
        let tmp = Temp::new();
        let mut child = Command::new("sleep")
            .arg("30")
            .current_dir(&tmp.0)
            .spawn()
            .unwrap();
        let scan = scan_processes();
        let _ = child.kill();
        let _ = child.wait();
        let scan = scan.unwrap();
        let user = scan.user_of(&tmp.0).expect("sleep's cwd is in use");
        assert_eq!(user.pid, child.id());
    }

    #[test]
    fn parses_lsof_ps_prs_and_porcelain() {
        let lsof =
            "p42\ncsleep\nfcwd\nn/work/wt\nf3\nn/work/wt/target/x (deleted)\nf4\nnlocalhost:22\n";
        let uses = parse_lsof(lsof);
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[1].path, PathBuf::from("/work/wt/target/x"));
        assert_eq!(uses[0].command, "sleep");

        let cwd: BTreeMap<u32, PathBuf> = [(7, PathBuf::from("/work/wt"))].into();
        let ps =
            "  7 python3 sweep.py --cli=target/release/fission_cli ../other/x\n  8 /bin/zsh -l\n";
        let uses = parse_ps(ps, &cwd);
        let paths: Vec<_> = uses.iter().map(|u| u.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/work/wt/target/release/fission_cli"),
                PathBuf::from("/work/other/x"),
                PathBuf::from("/bin/zsh"),
            ]
        );

        let prs = parse_prs("5\tCLOSED\tfeat/a\n9\tMERGED\tfeat/a\n7\tOPEN\tfeat/b\nbad\n");
        assert_eq!(
            prs["feat/a"],
            Pr {
                number: 9,
                state: PrState::Merged
            }
        );
        assert_eq!(prs["feat/b"].state, PrState::Open);

        let porcelain = "worktree /r\nHEAD aa\nbranch refs/heads/main\n\nworktree /w\nHEAD bb\ndetached\nlocked reason\n\nworktree /gone\nHEAD cc\nbranch refs/heads/x/y\nprunable gitdir file points to non-existent location\n";
        let list = parse_worktrees(porcelain);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert!(list[1].branch.is_none() && list[1].locked);
        assert!(list[2].prunable);
        assert_eq!(list[2].branch.as_deref(), Some("x/y"));
    }

    #[test]
    fn options_default_to_a_safe_report() {
        let o = Options::parse(&[]).unwrap();
        assert!(!o.apply && !o.include_main_target && o.use_gh);
        assert_eq!(o.min_age_hours, 24);
        let o = Options::parse(&["--apply".into(), "--min-age-hours".into(), "0".into()]).unwrap();
        assert!(o.apply);
        assert_eq!(o.min_age_hours, 0);
        assert!(Options::parse(&["--force".into()]).is_err());
    }
}
