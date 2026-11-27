//! Process-crash and recovery boundary verification for issue #1282.
//!
//! These tests exercise injected failures, simulated crashed writer state
//! (abandoned locks and truncated temporary staging files), and bounded
//! inspection limits. They verify process-crash recovery of atomic single-file
//! replacement and do not claim proof of survival across unbuffered physical
//! power loss.

use fission_project::{
    CommitDurability, CommitPrecondition, LockLiveness, MAX_INSPECTED_TEMP_FILES, Project,
    ProjectError, ProjectLock, ProjectStateStatus, StaleLockConfirmation, inspect_recovery,
    recover_abandoned_temps, remove_stale_lock,
};

fn sample_project(name: &str) -> Project {
    let mut p: Project =
        serde_json::from_str(r#"{"schema_version":2,"binary_hash":"fixture-hash"}"#).unwrap();
    p.set_name(0x401000, name);
    p
}

#[test]
fn successful_commit_reports_expected_platform_durability_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.bin.fission.json");

    let initial = sample_project("entry_fn");
    let receipt = initial
        .commit_checked_with_receipt(&path, CommitPrecondition::IfAbsent)
        .unwrap();

    #[cfg(unix)]
    assert_eq!(
        receipt.durability,
        CommitDurability::AtomicAndDirectorySynced
    );
    #[cfg(not(unix))]
    assert_eq!(
        receipt.durability,
        CommitDurability::AtomicOnlyDirSyncUnsupported
    );

    let inspection = inspect_recovery(&path).unwrap();
    assert!(inspection.lock.is_none());
    assert!(inspection.temp_files.is_empty());
    assert!(!inspection.temp_files_truncated);
    assert!(matches!(
        inspection.project_state,
        ProjectStateStatus::Valid {
            schema_version: 2,
            ..
        }
    ));
}

#[test]
fn crashed_writer_recovery_protects_unknown_and_active_locks_and_recovers_valid_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.bin.fission.json");
    let unrelated = dir.path().join("other.fission.json");
    std::fs::write(&unrelated, b"keep-me").unwrap();

    let committed = sample_project("committed_v1");
    let rev0 = committed.create_if_absent(&path).unwrap();

    // Simulate a crashed prior writer that left a lock file with foreign PID
    // + owner token and a partially written temporary file before rename.
    let lock_path = dir.path().join("sample.bin.fission.json.lock");
    std::fs::write(
        &lock_path,
        "pid=424242\nprocess_nonce=deadbeef-1\nowner=crash-owner-7\ncreated_unix_ms=1700000000000\n",
    )
    .unwrap();
    let crashed_temp = dir
        .path()
        .join("sample.bin.fission.json.424242.crash-owner-7.9.tmp");
    std::fs::write(&crashed_temp, br#"{"schema_version":2,"partial":true"#).unwrap();

    // Reading the project still returns the complete valid committed version,
    // never a mix of old state and the truncated temporary file.
    assert_eq!(Project::read(&path).unwrap().unwrap(), committed);

    let inspection = inspect_recovery(&path).unwrap();
    assert!(matches!(
        inspection.project_state,
        ProjectStateStatus::Valid { .. }
    ));
    let inspected_lock = inspection.lock.expect("crashed lock discovered");
    assert_eq!(inspected_lock.liveness, LockLiveness::Unknown);
    assert_eq!(inspected_lock.identity.pid, Some(424242));
    assert_eq!(
        inspected_lock.identity.owner_token.as_deref(),
        Some("crash-owner-7")
    );
    assert_eq!(inspection.temp_files.len(), 1);
    assert!(inspection.temp_files[0].owned_by_lock);

    // Temp cleanup skips files still owned by an existing lock.
    let cleanup_while_locked = recover_abandoned_temps(&path).unwrap();
    assert!(cleanup_while_locked.removed_temp_files.is_empty());
    assert_eq!(
        cleanup_while_locked.skipped_temp_files,
        vec![crashed_temp.clone()]
    );

    // Unconfirmed stale lock removal is refused for Unknown liveness.
    let err = remove_stale_lock(
        &path,
        &StaleLockConfirmation {
            expected_content_blake3: inspected_lock.content_blake3.clone(),
            operator_confirmed: false,
        },
    )
    .expect_err("must require operator_confirmed for Unknown lock");
    assert!(matches!(err, ProjectError::RecoveryRefused { .. }));
    assert!(lock_path.exists());

    // Digest mismatch (e.g. another writer replaced the lock) is also refused.
    let err = remove_stale_lock(
        &path,
        &StaleLockConfirmation {
            expected_content_blake3: "0".repeat(64),
            operator_confirmed: true,
        },
    )
    .expect_err("must reject stale digest");
    assert!(matches!(err, ProjectError::RecoveryRefused { .. }));
    assert!(lock_path.exists());

    // Confirmed stale lock removal succeeds, then temp recovery deletes only
    // the abandoned staging file and leaves both project and unrelated files intact.
    assert!(
        remove_stale_lock(
            &path,
            &StaleLockConfirmation {
                expected_content_blake3: inspected_lock.content_blake3,
                operator_confirmed: true,
            },
        )
        .unwrap()
    );
    assert!(!lock_path.exists());

    let cleanup = recover_abandoned_temps(&path).unwrap();
    assert_eq!(cleanup.removed_temp_files, vec![crashed_temp.clone()]);
    assert!(!crashed_temp.exists());
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"keep-me");
    assert_eq!(Project::read(&path).unwrap().unwrap(), committed);

    // Active lock in the same process can never be removed through recovery,
    // even with matching digest and operator_confirmed=true.
    let active_guard = ProjectLock::acquire(&path).unwrap();
    let active_inspection = inspect_recovery(&path).unwrap();
    let active_lock = active_inspection.lock.expect("active lock visible");
    assert_eq!(active_lock.liveness, LockLiveness::CurrentProcessActive);
    let err = remove_stale_lock(
        &path,
        &StaleLockConfirmation {
            expected_content_blake3: active_lock.content_blake3,
            operator_confirmed: true,
        },
    )
    .expect_err("must refuse removing an active in-process lock");
    assert!(matches!(err, ProjectError::RecoveryRefused { .. }));
    active_guard.release().unwrap();

    // Checked commit resumes normally once recovered.
    let next = sample_project("committed_v2");
    let _ = next.write_if_revision(&path, &rev0).unwrap();
    assert_eq!(Project::read(&path).unwrap().unwrap(), next);
}

#[test]
fn recovery_inspection_bounds_temp_file_enumeration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bounded.fission.json");
    sample_project("v1").create_if_absent(&path).unwrap();

    for idx in 0..(MAX_INSPECTED_TEMP_FILES + 5) {
        let orphan = dir
            .path()
            .join(format!("bounded.fission.json.99999.orphan-{idx}.{idx}.tmp"));
        std::fs::write(&orphan, b"{}").unwrap();
    }

    let inspection = inspect_recovery(&path).unwrap();
    assert_eq!(inspection.temp_files.len(), MAX_INSPECTED_TEMP_FILES);
    assert!(inspection.temp_files_truncated);
}
