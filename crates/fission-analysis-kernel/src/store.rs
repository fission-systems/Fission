//! The evidence ledger on disk.
//!
//! One JSON file, replaced atomically. It holds claims only; analyst decisions
//! stay in `.fission.json`. Concurrent writers are not coordinated -- the last
//! rename wins -- which the persistent store of #1368 M2 has to solve.

use crate::KernelError;
use fission_evidence::EvidenceLedger;
use std::io::Write;
use std::path::Path;

fn io_error(path: &Path) -> impl Fn(std::io::Error) -> KernelError {
    let path = path.to_path_buf();
    move |source| KernelError::Io {
        path: path.clone(),
        source,
    }
}

/// The ledger at `path`, or an empty one if no file exists yet.
pub fn load_or_empty(path: &Path) -> Result<EvidenceLedger, KernelError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(EvidenceLedger::from_json(&text)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(EvidenceLedger::default()),
        Err(error) => Err(io_error(path)(error)),
    }
}

/// The ledger at `path`, which must exist.
pub fn load(path: &Path) -> Result<EvidenceLedger, KernelError> {
    let text = std::fs::read_to_string(path).map_err(io_error(path))?;
    Ok(EvidenceLedger::from_json(&text)?)
}

/// Replace the file at `path`: a reader sees the old ledger or the new one.
pub fn save(path: &Path, ledger: &EvidenceLedger) -> Result<(), KernelError> {
    write_atomically(path, ledger.to_json()?.as_bytes())
}

/// Write `contents` to a temporary file beside `path`, then rename it over.
pub fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), KernelError> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(name);
    let written = std::fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(contents)?;
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|error| {
            // Best effort: the original error is the one worth reporting.
            let _ = std::fs::remove_file(&temporary);
            io_error(path)(error)
        })
}
