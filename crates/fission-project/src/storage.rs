//! Cooperative writer exclusion, revision tokens, and atomic replacement of one project file.
//!
//! # What a checked commit guarantees
//!
//! The guarantee is scoped to *cooperating writers*: every writer that commits
//! through [`ProjectLock`] (directly, or via `Project::commit_*` /
//! `Project::write_*`). Between cooperating writers a checked commit is a
//! compare-and-swap: the comparison and the replacement both happen while the
//! writer holds the lock, so no other cooperating writer can commit in
//! between, and a commit based on a stale [`ProjectRevision`] is rejected with
//! [`ProjectError::Conflict`] instead of losing the other writer's update.
//! The lock is an exclusively created `<project>.lock` file; a writer
//! re-checks that the lock file is still its own immediately before comparing
//! and refuses to commit ([`ProjectError::LockLost`]) if it was removed or
//! replaced.
//!
//! Writers that ignore the lock (editors, `cp`, sync tools) get less, because
//! no portable filesystem primitive renames *only if* the destination still
//! holds given bytes:
//!
//! - An external change that completes before the comparison is detected.
//!   The comparison runs after the new bytes are already durable in the
//!   temporary file, so the remaining window is one read plus one rename.
//! - An external change that lands between the comparison and the rename is
//!   overwritten. Atomic replacement (readers see the old or the new file,
//!   never a mix) is not atomic compare-and-replace.
//! - Create-if-absent (including a checked commit against an absent token) is
//!   atomic even against external writers: the new file is published with an
//!   exclusive hard link, which fails if anything exists at the path. On a
//!   filesystem without hard links it falls back to rename and has the same
//!   window as above.
//! - An external in-place writer can expose a partially written file to a
//!   lock-free reader; the revision of what was read then differs from the
//!   final state, so a commit based on it is rejected.
//!
//! # Revision equality and ABA
//!
//! Revision equality is decided by a blake3 digest of the whole file, its
//! length, and filesystem identity/timestamps. The digest alone is what makes
//! the check sound: the file is the complete project state, so if the bytes
//! are identical the stale writer's edit applies to exactly the state it read
//! and nothing is lost. The metadata only adds conflicts (a rewrite with
//! identical bytes usually gets a new inode or timestamp and is rejected),
//! which is conservative; it is not relied on, because inode numbers can be
//! reused and timestamps can be coarse.
//!
//! # Platforms
//!
//! Exclusive create, same-directory rename, and hard links are atomic on local
//! POSIX filesystems (Linux, macOS) and NTFS. On network filesystems these
//! guarantees depend on the server and client (exclusive create and
//! close-to-open consistency); they are not tested there.
use super::*;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Points inside a commit where tests interleave an adversarial writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitStage {
    /// The new bytes are durable in the temporary file; nothing is compared yet.
    TempDurable,
    /// The precondition has been evaluated and accepted.
    PreconditionChecked,
    /// The new file has been put in place; its token is not yet computed.
    Replaced,
}

#[cfg(test)]
type CommitHook = Box<dyn FnMut(CommitStage, &Path)>;

#[cfg(test)]
thread_local! {
    static COMMIT_HOOK: std::cell::RefCell<Option<CommitHook>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static HARD_LINKS_UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Make create-if-absent take its no-hard-link fallback on this thread.
#[cfg(test)]
pub(crate) fn set_hard_links_unsupported(unsupported: bool) {
    HARD_LINKS_UNSUPPORTED.with(|flag| flag.set(unsupported));
}

fn hard_link(original: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if HARD_LINKS_UNSUPPORTED.with(std::cell::Cell::get) {
        return Err(std::io::Error::from(std::io::ErrorKind::Unsupported));
    }
    std::fs::hard_link(original, link)
}

/// Install a test-only hook called at every [`CommitStage`] on this thread.
#[cfg(test)]
pub(crate) fn set_commit_hook(hook: Option<CommitHook>) {
    COMMIT_HOOK.with(|slot| *slot.borrow_mut() = hook);
}

#[inline]
fn commit_stage(stage: CommitStage, path: &Path) {
    #[cfg(test)]
    {
        // Take the hook out while it runs so it may itself commit.
        if let Some(mut hook) = COMMIT_HOOK.with(|slot| slot.borrow_mut().take()) {
            hook(stage, path);
            COMMIT_HOOK.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    *slot = Some(hook);
                }
            });
        }
    }
    #[cfg(not(test))]
    let _ = (stage, path);
}

/// Opaque persisted state token acquired when reading or committing a project file.
///
/// Captures absence separately from presence. For a present file it combines
/// a full blake3 digest of the persisted bytes with OS file identity and
/// timestamps; see the module documentation for what equality does and does
/// not establish.
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
    /// `None` for a commit whose file was replaced before it could be
    /// observed: such a token never equals an observed state, so the next
    /// checked commit against it conflicts.
    identity: Option<FileIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
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

impl FileIdentity {
    fn of(meta: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                dev: meta.dev(),
                ino: meta.ino(),
                ctime: meta.ctime(),
                ctime_nsec: meta.ctime_nsec(),
                mtime: meta.mtime(),
                mtime_nsec: meta.mtime_nsec(),
            }
        }
        #[cfg(not(unix))]
        {
            let to_nanos = |st: Option<std::time::SystemTime>| {
                st.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
            };
            Self {
                modified_nanos: to_nanos(meta.modified().ok()),
                created_nanos: to_nanos(meta.created().ok()),
            }
        }
    }
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
                match &stamp.identity {
                    None => format!("present:{hex}(len={},superseded)", stamp.len),
                    #[cfg(unix)]
                    Some(identity) => {
                        format!("present:{hex}(ino={},len={})", identity.ino, stamp.len)
                    }
                    #[cfg(not(unix))]
                    Some(_) => format!("present:{hex}(len={})", stamp.len),
                }
            }
        }
    }

    fn present(digest: [u8; 32], len: u64, identity: Option<FileIdentity>) -> Self {
        Self(RevisionState::Present(Arc::new(PersistedFileStamp {
            digest,
            len,
            identity,
        })))
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
    /// An absent token is treated exactly like [`CommitPrecondition::IfAbsent`].
    IfRevision(ProjectRevision),
    /// Explicitly bypass compare-and-swap checking and replace unconditionally.
    Unconditional,
}

fn stamp_from_file(file: &mut File) -> std::io::Result<(ProjectRevision, Vec<u8>)> {
    let meta = file.metadata()?;
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.read_to_end(&mut bytes)?;
    let digest = *blake3::hash(&bytes).as_bytes();
    let revision =
        ProjectRevision::present(digest, bytes.len() as u64, Some(FileIdentity::of(&meta)));
    Ok((revision, bytes))
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
    let (revision, bytes) = stamp_from_file(&mut file).map_err(|source| ProjectError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some((bytes, revision)))
}

pub(crate) fn read_persisted_revision(path: &Path) -> Result<ProjectRevision, ProjectError> {
    Ok(read_persisted_bytes_and_revision(path)?
        .map_or_else(ProjectRevision::absent, |(_, rev)| rev))
}

/// Writer exclusion for one project file; see the module documentation.
pub struct ProjectLock {
    path: PathBuf,
    lock_path: PathBuf,
    /// Exact bytes this guard wrote into its lock file. Release and commit
    /// check them, so a guard whose lock was removed (and possibly re-taken
    /// by another writer) neither commits nor deletes the successor's lock.
    owner: String,
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
        let mut file = OpenOptions::new()
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
        static ACQUISITIONS: AtomicU64 = AtomicU64::new(0);
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let owner = format!(
            "pid={}\nowner={started:x}-{:x}\n",
            std::process::id(),
            ACQUISITIONS.fetch_add(1, Ordering::Relaxed)
        );
        if let Err(source) = file
            .write_all(owner.as_bytes())
            .and_then(|()| file.sync_all())
        {
            // This lock was created a moment ago by this call; nobody else
            // can own it yet.
            drop(file);
            let _ = std::fs::remove_file(&lock_path);
            return Err(fail(source));
        }
        Ok(Self {
            path: path.to_path_buf(),
            lock_path,
            owner,
            file: Some(file),
        })
    }

    /// Whether the lock file at `lock_path` is still the one this guard wrote.
    fn still_owned(&self) -> Result<bool, std::io::Error> {
        match std::fs::read(&self.lock_path) {
            Ok(bytes) => Ok(bytes == self.owner.as_bytes()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Read the project file and its persisted revision token while holding the writer lock.
    pub fn read_with_revision(&self) -> Result<(Option<Project>, ProjectRevision), ProjectError> {
        Project::read_with_revision(&self.path)
    }

    /// Atomically replace the project file after verifying `precondition` under the lock.
    ///
    /// Returns the [`ProjectRevision`] of exactly the bytes this call
    /// persisted, or [`ProjectError::Conflict`] without altering the file on
    /// mismatch. The module documentation states which writers this protects
    /// against.
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
        // "Still absent" has an atomic primitive; route it there however it
        // was spelled.
        let precondition = match precondition {
            CommitPrecondition::IfRevision(expected) if expected.is_absent() => {
                CommitPrecondition::IfAbsent
            }
            other => other,
        };

        let mut text =
            serde_json::to_string_pretty(project).map_err(|source| ProjectError::Parse {
                path: self.path.clone(),
                source,
            })?;
        text.push('\n');
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        let len = text.len() as u64;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let temp = PathBuf::from(name);
        // create_new prevents touching a file we did not create.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(fail)?;
        let result = self.publish(
            file,
            &temp,
            text.as_bytes(),
            permissions,
            &precondition,
            digest,
            len,
        );
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(
        &self,
        mut file: File,
        temp: &Path,
        text: &[u8],
        permissions: Option<std::fs::Permissions>,
        precondition: &CommitPrecondition,
        digest: [u8; 32],
        len: u64,
    ) -> Result<ProjectRevision, ProjectError> {
        let fail = |source| ProjectError::Write {
            path: self.path.clone(),
            source,
        };
        file.write_all(text).map_err(fail)?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions).map_err(fail)?;
        }
        file.sync_all().map_err(fail)?;
        commit_stage(CommitStage::TempDurable, &self.path);

        // Everything slow is done: compare as late as possible, so only a
        // read and a rename separate the comparison from the replacement.
        if !self.still_owned().map_err(fail)? {
            return Err(ProjectError::LockLost(self.lock_path.clone()));
        }
        let current = read_persisted_revision(&self.path)?;
        let conflict = |expected: ProjectRevision, current: ProjectRevision| {
            Err(ProjectError::Conflict {
                path: self.path.clone(),
                expected,
                current,
            })
        };
        match precondition {
            CommitPrecondition::IfAbsent if !current.is_absent() => {
                return conflict(ProjectRevision::absent(), current);
            }
            CommitPrecondition::IfRevision(expected) if current != *expected => {
                return conflict(expected.clone(), current);
            }
            _ => {}
        }
        commit_stage(CommitStage::PreconditionChecked, &self.path);

        // Windows cannot be relied on to rename a file this process holds
        // open; elsewhere the handle stays open to stamp the published inode.
        #[cfg(not(unix))]
        drop(file);
        let published_by_link = match precondition {
            CommitPrecondition::IfAbsent => match hard_link(temp, &self.path) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return conflict(
                        ProjectRevision::absent(),
                        read_persisted_revision(&self.path)?,
                    );
                }
                // No hard links on this filesystem: rename, with the
                // check-then-replace window documented above.
                Err(_) => false,
            },
            _ => false,
        };
        if published_by_link {
            // The commit has landed; a leftover temporary name is harmless.
            let _ = std::fs::remove_file(temp);
        } else {
            std::fs::rename(temp, &self.path).map_err(fail)?;
        }
        commit_stage(CommitStage::Replaced, &self.path);

        // The token must describe the bytes this call published, never
        // whatever is at the path by now: adopting a writer that landed after
        // the rename would let the caller's next checked commit overwrite it.
        #[cfg(unix)]
        let identity = file.metadata().ok().map(|meta| FileIdentity::of(&meta));
        #[cfg(not(unix))]
        let identity = match read_persisted_revision(&self.path) {
            Ok(ProjectRevision(RevisionState::Present(stamp)))
                if stamp.digest == digest && stamp.len == len =>
            {
                stamp.identity.clone()
            }
            _ => None,
        };
        Ok(ProjectRevision::present(digest, len, identity))
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
        // Never delete a lock another writer now holds. Check-then-remove is
        // not atomic, but it only matters after someone removed a live lock.
        if matches!(self.still_owned(), Ok(true)) {
            let _ = std::fs::remove_file(&self.lock_path);
        }
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

        // A byte-identical rewrite usually gets a new inode/timestamp and is
        // rejected, but soundness does not rest on that (inodes are reused,
        // clocks are coarse): if the token is indistinguishable, the file is
        // byte-for-byte the state the stale writer read, so nothing is lost.
        let fresh_dir = tempfile::tempdir().unwrap();
        let id_path = fresh_dir.path().join("identical.json");
        let t0 = project().create_if_absent(&id_path).unwrap();
        let mut modified = project();
        modified.set_name(0x4000, "temp");
        let t1 = modified.write_if_revision(&id_path, &t0).unwrap();
        let original = std::fs::read(&id_path).ok();
        let t2 = project().write_if_revision(&id_path, &t1).unwrap();
        let rewritten = std::fs::read(&id_path).unwrap();
        assert_ne!(original.as_deref(), Some(rewritten.as_slice()));
        match modified.write_if_revision(&id_path, &t0) {
            Err(ProjectError::Conflict { .. }) => {}
            Ok(_) => assert_eq!(t2, t0, "accepted only when indistinguishable"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn create_if_absent_without_hard_links_still_creates_and_rejects_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        set_hard_links_unsupported(true);
        let created = project().create_if_absent(&path);
        let again = project().create_if_absent(&path);
        set_hard_links_unsupported(false);
        let created = created.unwrap();
        assert_eq!(created, read_persisted_revision(&path).unwrap());
        assert!(matches!(again, Err(ProjectError::Conflict { .. })));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
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

    /// Bytes an external, lock-ignoring writer puts in place.
    fn external_bytes(tag: &str) -> Vec<u8> {
        format!(r#"{{"schema_version":2,"binary_hash":"fixture","names":{{"0x9999":"{tag}"}}}}"#)
            .into_bytes()
    }

    /// Replace `path` the way an editor saving atomically does (sibling + rename).
    fn external_atomic_replace(path: &Path, bytes: &[u8]) {
        let mut sibling = path.as_os_str().to_os_string();
        sibling.push(".external-editor");
        std::fs::write(&sibling, bytes).unwrap();
        std::fs::rename(&sibling, path).unwrap();
    }

    /// Run `commit` with `writer` interleaved at `at`, once.
    fn with_writer_at<T>(
        at: CommitStage,
        writer: impl FnOnce(&Path) + 'static,
        commit: impl FnOnce() -> T,
    ) -> T {
        let mut writer = Some(writer);
        set_commit_hook(Some(Box::new(move |stage, path| {
            if stage == at {
                if let Some(writer) = writer.take() {
                    writer(path);
                }
            }
        })));
        let out = commit();
        set_commit_hook(None);
        out
    }

    #[test]
    fn an_external_replace_while_the_new_bytes_are_written_is_detected() {
        // The comparison must happen after the slow part (serialise, write,
        // fsync), so an edit that completes during it is seen, not overwritten.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let rev0 = project().create_if_absent(&path).unwrap();
        let mut mine = project();
        mine.set_name(0x1000, "mine");
        let external = external_bytes("during-write");
        let ext = external.clone();
        let result = with_writer_at(
            CommitStage::TempDurable,
            move |p| external_atomic_replace(p, &ext),
            || mine.write_if_revision(&path, &rev0),
        );
        assert!(
            matches!(result, Err(ProjectError::Conflict { .. })),
            "external edit completed before the comparison was overwritten: {result:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), external);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn an_external_create_racing_create_if_absent_is_never_overwritten() {
        // Create-if-absent has a portable atomic primitive (an exclusive hard
        // link), so it holds even against writers that ignore the lock.
        for expect_absent_token in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p.json");
            let external = external_bytes("raced-create");
            let ext = external.clone();
            let result = with_writer_at(
                CommitStage::PreconditionChecked,
                move |p| std::fs::write(p, &ext).unwrap(),
                || {
                    if expect_absent_token {
                        project().write_if_revision(&path, &ProjectRevision::absent())
                    } else {
                        project().create_if_absent(&path)
                    }
                },
            );
            match result {
                Err(ProjectError::Conflict {
                    expected, current, ..
                }) => {
                    assert!(expected.is_absent());
                    assert!(!current.is_absent());
                }
                other => panic!("racing external create was overwritten: {other:?}"),
            }
            assert_eq!(std::fs::read(&path).unwrap(), external);
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn the_returned_token_describes_this_commit_not_a_later_replacement() {
        // If the token were re-read from the path after replacing, a writer
        // landing in between would be adopted as "ours" and the next checked
        // commit would silently overwrite it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let rev0 = project().create_if_absent(&path).unwrap();
        let mut mine = project();
        mine.set_name(0x1000, "mine");
        let external = external_bytes("after-replace");
        let ext = external.clone();
        let mine_rev = with_writer_at(
            CommitStage::Replaced,
            move |p| external_atomic_replace(p, &ext),
            || mine.write_if_revision(&path, &rev0),
        )
        .expect("the commit itself landed");
        assert_ne!(mine_rev, read_persisted_revision(&path).unwrap());
        let mut next = mine.clone();
        next.set_name(0x1000, "next");
        assert!(matches!(
            next.write_if_revision(&path, &mine_rev),
            Err(ProjectError::Conflict { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external);
    }

    #[test]
    fn the_returned_token_matches_a_fresh_read_when_nothing_intervenes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let rev0 = project().create_if_absent(&path).unwrap();
        assert_eq!(rev0, read_persisted_revision(&path).unwrap());
        let mut next = project();
        next.set_name(0x1000, "n");
        let rev1 = next.write_if_revision(&path, &rev0).unwrap();
        assert_eq!(rev1, read_persisted_revision(&path).unwrap());
        let (_, read_rev) = Project::read_with_revision(&path).unwrap();
        assert_eq!(rev1, read_rev);
    }

    #[test]
    fn documented_limit_an_external_replace_after_the_comparison_is_overwritten() {
        // Characterisation, not a goal: no portable primitive compares and
        // renames atomically, so a writer that ignores the lock and lands
        // between the comparison and the rename is lost. If this starts
        // failing, the guarantee got stronger: update docs/CLI.md.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let rev0 = project().create_if_absent(&path).unwrap();
        let mut mine = project();
        mine.set_name(0x1000, "mine");
        let result = with_writer_at(
            CommitStage::PreconditionChecked,
            move |p| external_atomic_replace(p, &external_bytes("lost")),
            || mine.write_if_revision(&path, &rev0),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(Project::read(&path).unwrap().unwrap(), mine);
    }

    #[test]
    fn a_writer_whose_lock_was_deleted_neither_commits_nor_frees_the_successor() {
        // Removing a live lock (mistaking it for a crashed writer's) lets a
        // second writer in. The displaced writer must not commit, and its
        // drop must not delete the successor's lock.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let rev0 = project().create_if_absent(&path).unwrap();
        let first = ProjectLock::acquire(&path).unwrap();
        std::fs::remove_file(&first.lock_path).unwrap();
        let second = ProjectLock::acquire(&path).unwrap();
        let mut stale = project();
        stale.set_name(0x1000, "displaced");
        let result = first.write_if_revision(&stale, &rev0);
        assert!(
            matches!(result, Err(ProjectError::LockLost(_))),
            "displaced writer committed: {result:?}"
        );
        assert_eq!(read_persisted_revision(&path).unwrap(), rev0);
        drop(first);
        assert!(
            matches!(ProjectLock::acquire(&path), Err(ProjectError::Locked(_))),
            "dropping the displaced guard released the successor's lock"
        );
        let mut fresh = project();
        fresh.set_name(0x1000, "successor");
        second.write_if_revision(&fresh, &rev0).unwrap();
        drop(second);
        assert!(ProjectLock::acquire(&path).is_ok());
    }

    #[test]
    fn cas_counter_child() {
        let Some(path) = std::env::var_os("FISSION_CAS_COUNTER_PROJECT") else {
            return;
        };
        let path = PathBuf::from(path);
        let iterations: u64 = std::env::var("FISSION_CAS_COUNTER_ITERATIONS")
            .unwrap()
            .parse()
            .unwrap();
        let go = PathBuf::from(std::env::var_os("FISSION_CAS_COUNTER_GO").unwrap());
        while !go.exists() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        for _ in 0..iterations {
            loop {
                // Optimistic: read without the lock, commit under it.
                let (current, rev) = Project::read_with_revision(&path).unwrap();
                let current = current.unwrap();
                let n: u64 = current.names[&0x1000].parse().unwrap();
                match current.commit_edit(&path, &rev, |p| {
                    p.set_name(0x1000, &(n + 1).to_string());
                    Ok(())
                }) {
                    Ok(_) => break,
                    Err(ProjectError::Conflict { .. }) => {}
                    Err(ProjectError::Locked(_)) => {
                        std::thread::sleep(std::time::Duration::from_micros(500));
                    }
                    Err(other) => panic!("unexpected error: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn cooperating_processes_lose_no_increment() {
        // Every process does read -> +1 -> checked commit, retrying on
        // Conflict/Locked. Any lost update leaves the counter short.
        const PROCESSES: u64 = 4;
        const ITERATIONS: u64 = 15;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("counter.json");
        let go = dir.path().join("go");
        let mut base = project();
        base.set_name(0x1000, "0");
        base.create_if_absent(&path).unwrap();
        let exe = std::env::current_exe().unwrap();
        let children: Vec<_> = (0..PROCESSES)
            .map(|_| {
                std::process::Command::new(&exe)
                    .args([
                        "--exact",
                        "storage::tests::cas_counter_child",
                        "--nocapture",
                    ])
                    .env("FISSION_CAS_COUNTER_PROJECT", &path)
                    .env("FISSION_CAS_COUNTER_ITERATIONS", ITERATIONS.to_string())
                    .env("FISSION_CAS_COUNTER_GO", &go)
                    .spawn()
                    .unwrap()
            })
            .collect();
        std::fs::write(&go, b"go").unwrap();
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        let last = Project::read(&path).unwrap().unwrap();
        let expected = PROCESSES * ITERATIONS;
        assert_eq!(last.names[&0x1000], expected.to_string());
        assert_eq!(last.history.revisions().len() as u64, expected);
        assert!(!dir.path().join("counter.json.lock").exists());
    }
}
