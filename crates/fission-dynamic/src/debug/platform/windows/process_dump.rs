//! Windows-specific adapter separating raw process-memory evidence from
//! reconstructed PE derivatives and rebuilt import tables.
//!
//! Canonical acquisition, provenance tracking, derivative lineage, and
//! no-clobber persistence live in [`crate::debug::forensics`].

use std::path::Path;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

use super::import_recon::ImportEntry;
use super::memory_map::{MemoryState, enumerate_memory_regions};
use crate::debug::forensics::{
    DerivedPeManifest, ImportCandidate, ModuleAcquisitionOptions, ModuleMemoryAcquisition,
    PeRebuildOptions, acquire_module_memory, default_acquisition_path, default_manifest_path,
    derived_artifact_from_unmanifested_pe, rebuild_imports_derived,
    reconstruct_pe_from_acquisition,
};
use crate::debug::memory::{MemoryError, MemoryManager, MemoryProtection, MemoryRegion};
use crate::debug::platform::PlatformMemory;

struct WinHandleMemoryAdapter {
    handle: HANDLE,
}

impl PlatformMemory for WinHandleMemoryAdapter {
    fn open_process(&mut self, _pid: u32) -> Result<(), MemoryError> {
        Ok(())
    }

    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut bytes_read = 0usize;
        // SAFETY: `self.handle` is a caller-supplied live process handle and `buffer` is valid for `buffer.len()` bytes.
        let res = unsafe {
            ReadProcessMemory(
                self.handle,
                address as *const std::ffi::c_void,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                buffer.len(),
                Some(&mut bytes_read),
            )
        };
        match res {
            Ok(()) => Ok(bytes_read.min(buffer.len())),
            Err(e) => match e.code().0 as u32 & 0xFFFF {
                5 => Err(MemoryError::AccessDenied { address }),
                6 => Err(MemoryError::ProcessExited { address }),
                299 | 487 => {
                    if bytes_read > 0 {
                        Ok(bytes_read.min(buffer.len()))
                    } else {
                        Err(MemoryError::Unmapped {
                            address,
                            size: buffer.len(),
                        })
                    }
                }
                _ => Err(MemoryError::ReadFailed {
                    address,
                    reason: format!("ReadProcessMemory failed: {e}"),
                }),
            },
        }
    }

    fn write(&self, _address: u64, _data: &[u8]) -> Result<usize, MemoryError> {
        Err(MemoryError::WriteFailed {
            address: 0,
            reason: "forensic acquisition backend is read-only".to_string(),
        })
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        Ok(enumerate_memory_regions(self.handle)
            .into_iter()
            .filter(|r| r.state == MemoryState::Commit)
            .map(|r| {
                let executable = r.is_executable();
                let writable = r.is_writable();
                MemoryRegion {
                    base_address: r.base_address,
                    size: r.size,
                    protection: MemoryProtection {
                        read: r.is_readable(),
                        write: writable,
                        execute: executable,
                    },
                    name: None,
                }
            })
            .collect())
    }

    fn is_open(&self) -> bool {
        !self.handle.is_invalid()
    }
}

/// Acquire bounded raw module memory evidence from a live Win32 process handle.
pub fn acquire_process_module(
    process_handle: HANDLE,
    base_address: u64,
    options: &ModuleAcquisitionOptions,
) -> Result<ModuleMemoryAcquisition, String> {
    let mut manager = MemoryManager::with_backend(WinHandleMemoryAdapter {
        handle: process_handle,
    });
    acquire_module_memory(&mut manager, base_address, options).map_err(|e| e.to_string())
}

/// Acquire raw memory evidence (`<output_path>.acquisition.json`) and derive a
/// reconstructed PE image (`output_path`) + provenance manifest
/// (`<output_path>.manifest.json`) without clobbering existing artifacts.
pub fn dump_process(
    process_handle: HANDLE,
    base_address: u64,
    output_path: &str,
) -> Result<DerivedPeManifest, String> {
    let out_path = Path::new(output_path);
    let acq_path = default_acquisition_path(out_path);
    let manifest_path = default_manifest_path(out_path);

    let acquisition = acquire_process_module(
        process_handle,
        base_address,
        &ModuleAcquisitionOptions::default(),
    )?;
    acquisition.save_no_clobber(&acq_path)?;

    let derived = reconstruct_pe_from_acquisition(&acquisition, &PeRebuildOptions::default())?;
    derived.save_no_clobber(out_path, &manifest_path)?;
    Ok(derived.manifest)
}

/// Derive a new PE artifact with rebuilt imports into `output_path` +
/// `<output_path>.manifest.json`, preserving `input_path` and any companion raw
/// acquisition untouched.
pub fn rebuild_imports(
    input_path: &str,
    output_path: &str,
    imports: &[ImportEntry],
    _original_base: u64,
) -> Result<DerivedPeManifest, String> {
    let in_path = Path::new(input_path);
    let out_path = Path::new(output_path);
    if in_path == out_path {
        return Err(
            "refusing to rebuild imports in place; supply a distinct derivative output_path"
                .to_string(),
        );
    }

    let file_data = std::fs::read(in_path).map_err(|e| e.to_string())?;
    let in_manifest_path = default_manifest_path(in_path);
    let parent = if in_manifest_path.exists() {
        let manifest_bytes = std::fs::read(&in_manifest_path).map_err(|e| e.to_string())?;
        let manifest: DerivedPeManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
        crate::debug::forensics::DerivedPeArtifact {
            bytes: file_data,
            manifest,
        }
    } else {
        derived_artifact_from_unmanifested_pe(file_data, input_path)
    };

    let candidates: Vec<ImportCandidate> = imports
        .iter()
        .map(|import| ImportCandidate {
            rva: import.rva,
            target_address: import.target_address,
            module_name: Some(import.module_name.clone()),
            function_name: import.function_name.clone(),
            ordinal: Some(import.ordinal),
            unresolved_reason: None,
        })
        .collect();

    let derived = rebuild_imports_derived(&parent, &candidates)?;
    let out_manifest_path = default_manifest_path(out_path);
    derived.save_no_clobber(out_path, &out_manifest_path)?;
    Ok(derived.manifest)
}
