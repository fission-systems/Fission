//! ELF dynamic linker scaffolding (cleanroom; no QEMU/vendor deps).
//!
//! Two modes for dynamic binaries:
//! 1. **HLE GOT** (default): patch JUMP_SLOT/GLOB_DAT to magic trampolines and
//!    emulate libc entry (`__libc_start_main`) without loading `ld.so`.
//! 2. **Interpreter**: when `FISSION_ENABLE_DYNLINK=1` and the host can open the
//!    `PT_INTERP` path (or `FISSION_LD_SO` override), map the interpreter into
//!    guest memory and transfer entry to it. Full glibc/musl ld.so still needs
//!    richer openat/mmap/read coverage — this is the structural path, not a
//!    claim of complete dynamic linking.

use crate::pcode::page_map::prot;
use crate::pcode::state::MachineState;
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the process image will resolve dynamic symbols / entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynlinkMode {
    /// Static binary (no PT_INTERP / no iat_symbols).
    #[default]
    Static,
    /// Dynamic binary using emulator GOT HLE (no host ld.so).
    HleGot,
    /// Mapped host interpreter; entry is interpreter entry + bias.
    Interpreter,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DynlinkInfo {
    pub mode: DynlinkMode,
    /// Guest path string from PT_INTERP (e.g. `/lib/ld-musl-x86_64.so.1`).
    pub interp_path: Option<String>,
    /// Host path actually loaded (if any).
    pub host_interp_path: Option<String>,
    /// Guest load base of the interpreter image.
    pub interp_base: u64,
    /// Guest entry of the interpreter (entry_point + bias).
    pub interp_entry: u64,
    /// Original main-binary entry (AT_ENTRY when using interpreter).
    pub main_entry: u64,
}

const PT_INTERP: u32 = 3;
const PT_LOAD: u32 = 1;
/// Preferred guest base for a PIE-style dynamic linker image.
const DEFAULT_INTERP_BASE: u64 = 0x0000_5555_5555_0000;

/// Read PT_INTERP path from raw ELF bytes (64-bit LE).
pub fn parse_pt_interp(data: &[u8]) -> Option<String> {
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] {
        return None;
    }
    let is_64 = data[4] == 2;
    let is_le = data[5] == 1;
    if !is_64 || !is_le {
        return None; // scaffold: x86-64 LE only
    }
    let phoff = u64::from_le_bytes(data[32..40].try_into().ok()?);
    let phentsize = u16::from_le_bytes(data[54..56].try_into().ok()?) as usize;
    let phnum = u16::from_le_bytes(data[56..58].try_into().ok()?) as usize;
    if phentsize < 56 {
        return None;
    }
    for i in 0..phnum {
        let off = phoff as usize + i * phentsize;
        if off + 56 > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().ok()?);
        if p_type != PT_INTERP {
            continue;
        }
        let p_offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().ok()?) as usize;
        let p_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().ok()?) as usize;
        if p_offset == 0 || p_filesz == 0 || p_offset + p_filesz > data.len() {
            return None;
        }
        let raw = &data[p_offset..p_offset + p_filesz];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let s = String::from_utf8_lossy(&raw[..end]).into_owned();
        if s.is_empty() {
            return None;
        }
        return Some(s);
    }
    None
}

pub(crate) fn host_interp_candidate(guest_path: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FISSION_LD_SO") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let pb = PathBuf::from(guest_path);
    if pb.is_file() {
        return Some(pb);
    }
    // Common musl/glibc names on a Linux host when the embedded path is absolute.
    None
}

pub(crate) fn dynlink_enabled() -> bool {
    matches!(
        std::env::var("FISSION_ENABLE_DYNLINK").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// True when GOT should be left for a host-mapped ld.so (opt-in interpreter mode).
pub fn should_skip_got_hle(binary: &LoadedBinary) -> bool {
    if !dynlink_enabled() {
        return false;
    }
    let Some(gpath) = parse_pt_interp(binary.inner().data.as_slice()) else {
        return false;
    };
    host_interp_candidate(&gpath).is_some()
}

/// Decide dynlink mode and optionally map the host interpreter into `state`.
///
/// Returns info; on `Interpreter` mode also maps PT_LOAD segments of the
/// interpreter and sets `interp_entry` for the process image entry override.
pub fn prepare_dynlink(
    state: &mut MachineState,
    binary: &LoadedBinary,
) -> Result<DynlinkInfo> {
    let data = binary.inner().data.as_slice();
    let main_entry = binary.inner().entry_point;
    let interp = parse_pt_interp(data);
    let has_got = !binary.inner().iat_symbols.is_empty();

    if interp.is_none() && !has_got {
        return Ok(DynlinkInfo {
            mode: DynlinkMode::Static,
            main_entry,
            ..Default::default()
        });
    }

    let guest_interp = interp.clone();
    if dynlink_enabled() {
        if let Some(ref gpath) = guest_interp {
            if let Some(host) = host_interp_candidate(gpath) {
                match map_interpreter(state, &host, DEFAULT_INTERP_BASE) {
                    Ok(mapped) => {
                        tracing::info!(
                            "dynlink: mapped interpreter {} at base=0x{:X} entry=0x{:X}",
                            host.display(),
                            mapped.base,
                            mapped.entry
                        );
                        return Ok(DynlinkInfo {
                            mode: DynlinkMode::Interpreter,
                            interp_path: guest_interp,
                            host_interp_path: Some(host.display().to_string()),
                            interp_base: mapped.base,
                            interp_entry: mapped.entry,
                            main_entry,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "dynlink: failed to map interpreter {}: {e:#}; falling back to HLE GOT",
                            host.display()
                        );
                    }
                }
            } else {
                tracing::debug!(
                    "dynlink: host cannot open PT_INTERP `{}` (set FISSION_LD_SO); HLE GOT",
                    gpath
                );
            }
        }
    }

    Ok(DynlinkInfo {
        mode: DynlinkMode::HleGot,
        interp_path: guest_interp,
        host_interp_path: None,
        interp_base: 0,
        interp_entry: 0,
        main_entry,
    })
}

struct MappedInterp {
    base: u64,
    entry: u64,
}

/// Map a host ELF interpreter into guest RAM at `preferred_base` (PIE-friendly).
fn map_interpreter(
    state: &mut MachineState,
    path: &Path,
    preferred_base: u64,
) -> Result<MappedInterp> {
    let interp_bin =
        LoadedBinary::from_file(path).with_context(|| format!("load interp {}", path.display()))?;
    let inner = interp_bin.inner();
    let data = inner.data.as_slice();
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] {
        anyhow::bail!("not an ELF interpreter");
    }
    let is_64 = data[4] == 2;
    let is_le = data[5] == 1;
    if !is_64 || !is_le {
        anyhow::bail!("only ELF64 LE interpreters supported in scaffold");
    }
    let e_entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
    let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
    let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;

    // Prefer ELF preferred vaddrs; if PIE (min vaddr 0), place at preferred_base.
    let mut min_vaddr = u64::MAX;
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        if off + 56 > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let vaddr = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
        min_vaddr = min_vaddr.min(vaddr);
    }
    if min_vaddr == u64::MAX {
        anyhow::bail!("interpreter has no PT_LOAD");
    }
    let bias = if min_vaddr == 0 {
        preferred_base
    } else {
        0
    };

    for i in 0..phnum {
        let off = phoff + i * phentsize;
        if off + 56 > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_flags = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        let p_offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap()) as usize;
        let p_vaddr = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().unwrap()) as usize;
        let p_memsz = u64::from_le_bytes(data[off + 40..off + 48].try_into().unwrap());
        if p_memsz == 0 {
            continue;
        }
        let guest_va = p_vaddr.saturating_add(bias);
        let mut page_prot = prot::VALID | prot::READ;
        if p_flags & 2 != 0 {
            page_prot |= prot::WRITE;
        }
        if p_flags & 1 != 0 {
            page_prot |= prot::EXEC;
        }
        let mut buf = vec![0u8; p_memsz as usize];
        if p_filesz > 0 && p_offset + p_filesz <= data.len() {
            let n = p_filesz.min(buf.len());
            buf[..n].copy_from_slice(&data[p_offset..p_offset + n]);
        }
        state
            .write_space(state.ram_space(), guest_va, &buf)
            .with_context(|| format!("map interp segment at 0x{guest_va:X}"))?;
        state
            .page_map
            .map_region(guest_va, p_memsz, page_prot, false);
    }

    Ok(MappedInterp {
        base: if bias != 0 { bias } else { min_vaddr },
        entry: e_entry.saturating_add(bias),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_interp_from_dyn_puts_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf");
        if !path.is_file() {
            return;
        }
        let data = std::fs::read(&path).unwrap();
        let interp = parse_pt_interp(&data).expect("PT_INTERP");
        assert!(
            interp.contains("ld-musl") || interp.contains("ld-linux"),
            "unexpected interp: {interp}"
        );
    }

    #[test]
    fn prepare_dynlink_defaults_to_hle_without_env() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf");
        if !path.is_file() {
            return;
        }
        // Ensure env is off for this unit test (Rust 2024: env mut is unsafe).
        unsafe {
            std::env::remove_var("FISSION_ENABLE_DYNLINK");
        }
        let binary = LoadedBinary::from_file(&path).unwrap();
        let mut state = MachineState::new();
        let info = prepare_dynlink(&mut state, &binary).unwrap();
        assert_eq!(info.mode, DynlinkMode::HleGot);
        assert!(info.interp_path.is_some());
    }
}
