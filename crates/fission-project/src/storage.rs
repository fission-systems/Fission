//! Cooperative writer exclusion and atomic replacement of one project file.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

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

    pub fn write(&self, project: &Project) -> Result<(), ProjectError> {
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
        result
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
        assert!(project().write(&path).is_err());
        guard.write(&project()).unwrap();
        assert_eq!(Project::read(&path).unwrap(), Some(project()));
        drop(guard);
        project().write(&path).unwrap();
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
        assert!(project().write(&path).is_err());
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
        project().write(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let original = std::fs::read(&target).unwrap();
        symlink(&target, &link).unwrap();
        assert!(project().write(&link).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), original);
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        project().write(&target).unwrap();
        assert_eq!(
            std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
