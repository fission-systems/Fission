//! Cooperative writer exclusion, revision tokens, and atomic replacement of one project file.
use super::*;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Opaque persisted state token acquired when reading or committing a project file.
///
/// Captures absence separately from presence, and for present files combines a
/// full cryptographic digest (`blake3`) of the persisted bytes with OS file
/// identity and nanosecond modification/change metadata so reverts, rewrites,
/// undo/redo, external edits, and delete/recreate cycles remain distinguishable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRevision(RevisionState);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RevisionState {
    Absent,
    Present(Arc<PersistedFileStamp>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersistedFileStamp {
    digest: [u8; 32],
    len: u64,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    ctime: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
    #[cfg(unix)]
    mtime: i64,
    #[cfg(unix)]
    mtime_nsec: i64,
    #[cfg(not(unix))]
    modified_nanos: Option<u128>,
    #[cfg(not(unix))]
    created_nanos: Option<u128>,
}

impl ProjectRevision {
    /// Revision token representing an absent project file on disk.
    pub fn absent() -> Self {
        Self(RevisionState::Absent)
    }

    pub fn is_absent(&self) -> bool {
        matches!(self.0, RevisionState::Absent)
    }

    /// Short human-readable diagnostic summary of this revision token.
    pub fn summary(&self) -> String {
        match &self.0 {
            RevisionState::Absent => "absent".to_string(),
            RevisionState::Present(stamp) => {
                let mut hex = String::with_capacity(16);
                for byte in &stamp.digest[..8] {
                    use fmt::Write as _;
                    let _ = write!(&mut hex, "{byte:02x}");
                }
                #[cfg(unix)]
                {
                    format!("present:{hex}(ino={},len={})", stamp.ino, stamp.len)
                }
                #[cfg(not(unix))]
                {
                    format!("present:{hex}(len={})", stamp.len)
                }
            }
        }
    }
}

impl fmt::Display for ProjectRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

/// Precondition checked under [`ProjectLock`] before replacing the persisted project file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitPrecondition {
    /// Create the file only if no project file exists yet.
    IfAbsent,
    /// Replace only if the current persisted state matches the expected token.
    IfRevision(ProjectRevision),
    /// Explicitly bypass compare-and-swap checking and replace unconditionally.
    Unconditional,
}

fn stamp_from_file(file: &mut File) -> std::io::Result<(PersistedFileStamp, Vec<u8>)> {
    let meta = file.metadata()?;
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.read_to_end(&mut bytes)?;
    let digest = *blake3::hash(&bytes).as_bytes();
    let len = bytes.len() as u64;
    #[cfg(unix)]
    let stamp = {
        use std::os::unix::fs::MetadataExt;
        PersistedFileStamp {
            digest,
            len,
            dev: meta.dev(),
            ino: meta.ino(),
            ctime: meta.ctime(),
            ctime_nsec: meta.ctime_nsec(),
            mtime: meta.mtime(),
            mtime_nsec: meta.mtime_nsec(),
        }
    };
    #[cfg(not(unix))]
    let stamp = {
        let to_nanos = |st: Option<std::time::SystemTime>| {
            st.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
        };
        PersistedFileStamp {
            digest,
            len,
            modified_nanos: to_nanos(meta.modified().ok()),
            created_nanos: to_nanos(meta.created().ok()),
        }
    };
    Ok((stamp, bytes))
}

pub(crate) fn read_persisted_bytes_and_revision(
    path: &Path,
) -> Result<Option<(Vec<u8>, ProjectRevision)>, ProjectError> {
    let mut file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ProjectError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let (stamp, bytes) = stamp_from_file(&mut file).map_err(|source| ProjectError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some((
        bytes,
        ProjectRevision(RevisionState::Present(Arc::new(stamp))),
    )))
}

pub(crate) fn read_persisted_revision(path: &Path) -> Result<ProjectRevision, ProjectError> {
    Ok(read_persisted_bytes_and_revision(path)?
        .map_or_else(ProjectRevision::absent, |(_, rev)| rev))
}

pub struct ProjectLock {
    path: PathBuf,
    lock_path: PathBuf,
    file: Option<File>,
}

impl ProjectLock {
    /// Hold this guard across read, mutation and write. A crashed writer's
    /// lock is deliberately not guessed away: stop the writer before removing
    /// a stale lock. Readers need no lock because replacement is atomic.
    pub fn acquire(path: &Path) -> Result<Self, ProjectError> {
        let fail = |source| ProjectError::Write {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::AlreadyExists {
                    ProjectError::Locked(lock_path.clone())
                } else {
                    fail(source)
                }
            })?;
        let mut guard = Self {
            path: path.to_path_buf(),
            lock_path,
            file: Some(file),
        };
        writeln!(
            guard.file.as_mut().expect("lock file is open"),
            "pid={}",
            std::process::id()
        )
        .map_err(fail)?;
        Ok(guard)
    }

    /// Read the project file and its persisted revision token while holding the writer lock.
    pub fn read_with_revision(&self) -> Result<(Option<Project>, ProjectRevision), ProjectError> {
        Project::read_with_revision(&self.path)
    }

    /// Atomically replace the project file after verifying `precondition` under the lock.
    ///
    /// Returns the newly persisted [`ProjectRevision`] on success or
    /// [`ProjectError::Conflict`] without altering the file on mismatch.
    pub fn write_checked(
        &self,
        project: &Project,
        precondition: CommitPrecondition,
    ) -> Result<ProjectRevision, ProjectError> {
        project.history.validate()?;
        let fail = |source| ProjectError::Write {
            path: self.path.clone(),
            source,
        };
        let permissions = match std::fs::symlink_metadata(&self.path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(fail(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "project path must not be a symbolic link",
                )));
            }
            Ok(meta) => Some(meta.permissions()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(fail(e)),
        };

        let current = read_persisted_revision(&self.path)?;
        match precondition {
            CommitPrecondition::IfAbsent => {
                if !current.is_absent() {
                    return Err(ProjectError::Conflict {
                        path: self.path.clone(),
                        expected: ProjectRevision::absent(),
                        current,
                    });
                }
            }
            CommitPrecondition::IfRevision(expected) => {
                if current != expected {
                    return Err(ProjectError::Conflict {
                        path: self.path.clone(),
                        expected,
                        current,
                    });
                }
            }
            CommitPrecondition::Unconditional => {}
        }

        let mut text =
            serde_json::to_string_pretty(project).map_err(|source| ProjectError::Parse {
                path: self.path.clone(),
                source,
            })?;
        text.push('\n');
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let temp = PathBuf::from(name);
        // create_new prevents touching a file we did not create.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(fail)?;
        let result = (|| {
            file.write_all(text.as_bytes()).map_err(fail)?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions).map_err(fail)?;
            }
            file.sync_all().map_err(fail)?;
            drop(file);
            std::fs::rename(&temp, &self.path).map_err(fail)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result?;
        read_persisted_revision(&self.path)
    }

    /// Create the project file only if it does not already exist.
    pub fn write_if_absent(&self, project: &Project) -> Result<ProjectRevision, ProjectError> {
        self.write_checked(project, CommitPrecondition::IfAbsent)
    }

    /// Commit `project` only if the persisted file still matches `expected`.
    pub fn write_if_revision(
        &self,
        project: &Project,
        expected: &ProjectRevision,
    ) -> Result<ProjectRevision, ProjectError> {
        self.write_checked(project, CommitPrecondition::IfRevision(expected.clone()))
    }

    /// Explicitly overwrite any existing file without comparing persisted revisions.
    pub fn write_unconditional(&self, project: &Project) -> Result<(), ProjectError> {
        self.write_checked(project, CommitPrecondition::Unconditional)?;
        Ok(())
    }

    #[deprecated(
        note = "use write_if_revision, write_if_absent, or write_checked (or write_unconditional when overwriting unconditionally)"
    )]
    pub fn write(&self, project: &Project) -> Result<(), ProjectError> {
        self.write_unconditional(project)
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // Close before deletion, including on platforms that disallow unlinking
        // an open handle.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        serde_json::from_str(r#"{"schema_version":2,"binary_hash":"fixture"}"#).unwrap()
    }

    #[test]
    fn a_lock_excludes_another_writer_and_is_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/project.json");
        let guard = ProjectLock::acquire(&path).unwrap();
        assert!(matches!(
            ProjectLock::acquire(&path),
            Err(ProjectError::Locked(_))
        ));
        assert!(project().write_unconditional(&path).is_err());
        guard.write_if_absent(&project()).unwrap();
        assert_eq!(Project::read(&path).unwrap(), Some(project()));
        drop(guard);
        project().write_unconditional(&path).unwrap();
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn failed_replacement_cleans_owned_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.json");
        // rename cannot replace a nonempty directory on any supported OS.
        std::fs::create_dir(&path).unwrap();
        let original = path.join("untouched");
        std::fs::write(&original, "original bytes").unwrap();
        assert!(project().write_unconditional(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&original).unwrap(),
            "original bytes"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_is_not_replaced_and_original_permissions_survive() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.json");
        let link = dir.path().join("link.json");
        project().create_if_absent(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let original = std::fs::read(&target).unwrap();
        symlink(&target, &link).unwrap();
        assert!(project().write_unconditional(&link).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        project().write_unconditional(&target).unwrap();
        assert_eq!(
            std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn create_if_absent_cas_and_unconditional_preconditions_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.json");

        let (missing, absent_rev) = Project::read_with_revision(&path).unwrap();
        assert!(missing.is_none());
        assert!(absent_rev.is_absent());

        let mut first = project();
        first.set_name(0x1000, "alpha");
        let rev1 = first.create_if_absent(&path).unwrap();
        assert!(!rev1.is_absent());
        let saved_bytes = std::fs::read(&path).unwrap();

        // A second create_if_absent (or commit expecting absent_rev) is rejected with Conflict.
        let mut second = project();
        second.set_name(0x1000, "beta");
        let err = second
            .create_if_absent(&path)
            .expect_err("create_if_absent must fail when file exists");
        match err {
            ProjectError::Conflict {
                expected, current, ..
            } => {
                assert!(expected.is_absent());
                assert_eq!(current, rev1);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);

        let err2 = second
            .write_if_revision(&path, &absent_rev)
            .expect_err("absent token must fail once created");
        assert!(matches!(err2, ProjectError::Conflict { .. }));
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);

        // Explicit unconditional replacement succeeds even without expected token.
        second.write_unconditional(&path).unwrap();
        assert_eq!(
            Project::read(&path)
                .unwrap()
                .unwrap()
                .names
                .get(&0x1000)
                .map(String::as_str),
            Some("beta")
        );
    }

    #[test]
    fn two_stale_readers_reject_second_commit_for_disjoint_and_overlapping_edits() {
        for disjoint in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("shared.json");

            let mut base = project();
            base.set_name(0x1000, "initial");
            let rev0 = base.create_if_absent(&path).unwrap();

            // Client A and Client B both read rev0 without holding a lock.
            let (read_a, tok_a) = Project::read_with_revision(&path).unwrap();
            let (read_b, tok_b) = Project::read_with_revision(&path).unwrap();
            let p_a = read_a.unwrap();
            let p_b = read_b.unwrap();
            assert_eq!(tok_a, rev0);
            assert_eq!(tok_b, rev0);

            // Client A commits first through the single transactional API.
            let (committed_a, hist_a, rev1) = p_a
                .commit_edit(&path, &tok_a, |p| {
                    p.set_name(0x1000, "client_a");
                    Ok(())
                })
                .expect("first commit succeeds");
            assert_eq!(hist_a, Some(1));
            let bytes_after_a = std::fs::read(&path).unwrap();

            // Client B makes a disjoint OR overlapping edit from stale tok_b and tries to commit.
            let err = p_b
                .commit_edit(&path, &tok_b, |p| {
                    if disjoint {
                        p.set_comment(0x2000, "disjoint_b");
                    } else {
                        p.set_name(0x1000, "overlap_b");
                    }
                    Ok(())
                })
                .expect_err("stale commit must be rejected");
            let current_from_conflict = match err {
                ProjectError::Conflict {
                    expected, current, ..
                } => {
                    assert_eq!(expected, rev0);
                    assert_eq!(current, rev1);
                    current
                }
                other => panic!("expected Conflict, got {other:?}"),
            };

            // First committed bytes and history remain 100% intact.
            assert_eq!(std::fs::read(&path).unwrap(), bytes_after_a);
            assert_eq!(Project::read(&path).unwrap().unwrap(), committed_a);

            // Re-reading at current token allows Client B to commit cleanly.
            let (latest, latest_tok) = Project::read_with_revision(&path).unwrap();
            assert_eq!(latest_tok, current_from_conflict);
            let (final_project, hist_b, _) = latest
                .unwrap()
                .commit_edit(&path, &latest_tok, |p| {
                    if disjoint {
                        p.set_comment(0x2000, "disjoint_b");
                    } else {
                        p.set_name(0x1000, "overlap_b");
                    }
                    Ok(())
                })
                .expect("fresh rebase commit succeeds");
            assert_eq!(hist_b, Some(2));
            assert_eq!(Project::read(&path).unwrap().unwrap(), final_project);
        }
    }

    #[test]
    fn no_aba_after_undo_redo_branch_replacement_or_identical_byte_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aba.json");

        let mut base = project();
        base.set_name(0x1000, "orig");
        let rev0 = base.create_if_absent(&path).unwrap();
        let (stale_reader, stale_tok) = Project::read_with_revision(&path).unwrap();
        let stale_reader = stale_reader.unwrap();
        assert_eq!(stale_tok, rev0);

        // Recorded edit -> undo -> redo -> branch replacement.
        let (p1, _, rev1) = stale_reader
            .commit_edit(&path, &rev0, |p| {
                p.set_name(0x1000, "v1");
                Ok(())
            })
            .unwrap();
        let (p2, _, rev2) = p1.commit_undo(&path, &rev1).unwrap();
        assert_ne!(rev2, rev0);
        let (p3, _, rev3) = p2.commit_redo(&path, &rev2).unwrap();
        assert_ne!(rev3, rev0);
        let (p4, _, rev4) = p3.commit_undo(&path, &rev3).unwrap();
        let (_, _, rev5) = p4
            .commit_edit(&path, &rev4, |p| {
                p.set_comment(0x1000, "branched");
                Ok(())
            })
            .unwrap();
        assert_ne!(rev5, rev0);

        // Even after undo/redo/branching, stale_tok from initial state is rejected.
        let err = stale_reader
            .commit_edit(&path, &stale_tok, |p| {
                p.set_name(0x1000, "clobber");
                Ok(())
            })
            .expect_err("stale token after undo/redo must fail");
        assert!(matches!(err, ProjectError::Conflict { .. }));

        // Low-level identical-byte replacement without history changes also advances token (new inode/stamp).
        let fresh_dir = tempfile::tempdir().unwrap();
        let id_path = fresh_dir.path().join("identical.json");
        let t0 = project().create_if_absent(&id_path).unwrap();
        let mut modified = project();
        modified.set_name(0x4000, "temp");
        let t1 = modified.write_if_revision(&id_path, &t0).unwrap();
        // Rewrite exact original bytes via ProjectLock replacement.
        let t2 = project().write_if_revision(&id_path, &t1).unwrap();
        assert_ne!(t2, t0, "identical-byte atomic rewrite must not cause ABA");
        assert!(matches!(
            modified.write_if_revision(&id_path, &t0),
            Err(ProjectError::Conflict { .. })
        ));
    }

    #[test]
    fn external_edit_and_delete_recreate_reject_stale_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("external.json");

        let rev0 = project().create_if_absent(&path).unwrap();
        let (loaded, stale_tok) = Project::read_with_revision(&path).unwrap();
        let mut edited = loaded.unwrap();
        edited.set_name(0x1000, "stale");

        // 1. External edit replaces bytes directly on disk.
        let external_bytes =
            br#"{"schema_version":2,"binary_hash":"fixture","names":{"0x9999":"ext"}}"#;
        std::fs::write(&path, external_bytes).unwrap();
        let err = edited
            .write_if_revision(&path, &stale_tok)
            .expect_err("external edit must cause Conflict");
        assert!(matches!(err, ProjectError::Conflict { .. }));
        assert_eq!(std::fs::read(&path).unwrap(), external_bytes);

        // 2. External deletion makes persisted state absent; stale Present token is rejected.
        std::fs::remove_file(&path).unwrap();
        let err = edited
            .write_if_revision(&path, &rev0)
            .expect_err("deleted file must cause Conflict against present token");
        match err {
            ProjectError::Conflict {
                expected, current, ..
            } => {
                assert_eq!(expected, rev0);
                assert!(current.is_absent());
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!path.exists());

        // 3. Recreate file via atomic create with identical bytes after deleting it;
        // stale rev0 must still fail when inode/timestamp changed.
        let recreated_rev = project().create_if_absent(&path).unwrap();
        assert_ne!(recreated_rev, rev0);
        assert!(matches!(
            edited.write_if_revision(&path, &rev0),
            Err(ProjectError::Conflict { .. })
        ));
    }

    #[test]
    fn cas_race_child() {
        if std::env::var_os("FISSION_CAS_CHILD").is_none() {
            return;
        }
        let path = PathBuf::from(std::env::var_os("FISSION_CAS_PROJECT").unwrap());
        let ready = PathBuf::from(std::env::var_os("FISSION_CAS_READY").unwrap());
        let go = PathBuf::from(std::env::var_os("FISSION_CAS_GO").unwrap());
        let name = std::env::var("FISSION_CAS_NAME").unwrap();
        let result_path = PathBuf::from(std::env::var_os("FISSION_CAS_RESULT").unwrap());

        let (proj, rev) = Project::read_with_revision(&path).unwrap();
        let proj = proj.unwrap();
        std::fs::write(&ready, b"ready").unwrap();
        while !go.exists() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let outcome = loop {
            match proj.commit_edit(&path, &rev, |p| {
                p.set_name(0x1000, &name);
                Ok(())
            }) {
                Ok(_) => break format!("WIN:{name}"),
                Err(ProjectError::Locked(_)) => {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(ProjectError::Conflict { .. }) => break format!("CONFLICT:{name}"),
                Err(other) => panic!("unexpected error in child {name}: {other:?}"),
            }
        };
        std::fs::write(&result_path, outcome).unwrap();
    }

    #[test]
    fn concurrent_separate_processes_preserve_winner_and_reject_stale_loser() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("race.json");
        let go_path = dir.path().join("go");
        let ready_a = dir.path().join("ready_a");
        let ready_b = dir.path().join("ready_b");
        let res_a = dir.path().join("res_a");
        let res_b = dir.path().join("res_b");

        project().create_if_absent(&project_path).unwrap();
        let exe = std::env::current_exe().unwrap();

        let spawn_child = |ready: &Path, res: &Path, name: &str| {
            std::process::Command::new(&exe)
                .args(["--exact", "storage::tests::cas_race_child", "--nocapture"])
                .env("FISSION_CAS_CHILD", "1")
                .env("FISSION_CAS_PROJECT", &project_path)
                .env("FISSION_CAS_READY", ready)
                .env("FISSION_CAS_GO", &go_path)
                .env("FISSION_CAS_NAME", name)
                .env("FISSION_CAS_RESULT", res)
                .spawn()
                .expect("spawn child process")
        };

        let mut child_a = spawn_child(&ready_a, &res_a, "proc_a");
        let mut child_b = spawn_child(&ready_b, &res_b, "proc_b");

        while !ready_a.exists() || !ready_b.exists() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::fs::write(&go_path, b"go").unwrap();

        assert!(child_a.wait().unwrap().success());
        assert!(child_b.wait().unwrap().success());

        let out_a = std::fs::read_to_string(&res_a).unwrap();
        let out_b = std::fs::read_to_string(&res_b).unwrap();
        let (winner_name, loser_outcome) = if out_a.starts_with("WIN:") {
            assert!(out_b.starts_with("CONFLICT:"), "a={out_a} b={out_b}");
            ("proc_a", out_b)
        } else {
            assert!(out_a.starts_with("CONFLICT:"), "a={out_a} b={out_b}");
            assert!(out_b.starts_with("WIN:"), "a={out_a} b={out_b}");
            ("proc_b", out_a)
        };
        assert!(loser_outcome.starts_with("CONFLICT:"));

        let final_project = Project::read(&project_path).unwrap().unwrap();
        assert_eq!(
            final_project.names.get(&0x1000).map(String::as_str),
            Some(winner_name)
        );
        assert_eq!(final_project.history.revisions().len(), 1);
    }
}
