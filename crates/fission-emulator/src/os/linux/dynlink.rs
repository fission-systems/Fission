//! ELF dynamic linker scaffolding (cleanroom; no QEMU/vendor deps).
//!
//! Dynamic binaries use one of these paths:
//! 1. **HLE GOT** (default): patch JUMP_SLOT/GLOB_DAT to magic trampolines and
//!    emulate libc entry (`__libc_start_main`) without loading `ld.so`.
//! 2. **Interpreter**: when `FISSION_ENABLE_DYNLINK=1` and the host can open the
//!    `PT_INTERP` path (or `FISSION_LD_SO` override), map the interpreter into
//!    guest memory and transfer entry to it. Full glibc/musl ld.so still needs
//!    richer openat/mmap/read coverage — this is the structural path, not a
//!    claim of complete dynamic linking.
//!
//! AArch64 currently uses the HLE GOT path only. It applies `RELATIVE`,
//! `ABS64`, `GLOB_DAT`, and `JUMP_SLOT` relocations from the ELF dynamic table;
//! imported calls are then routed through the registered HLE procedures.
//! AArch64 interpreters and guest shared libraries are not loaded yet. This
//! path never searches host library directories by soname.

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
    /// Fission mini-dynlink loaded DT_NEEDED shared libs + BIND_NOW RELA.
    SharedLibs,
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
    /// Shared libraries loaded by the mini-dynlink loop (soname → guest base).
    #[serde(default)]
    pub loaded_libs: Vec<(String, u64)>,
    /// Whether DF_BIND_NOW / DT_FLAGS_1 NOW was applied eagerly.
    #[serde(default)]
    pub bind_now: bool,
    /// Global symbols from main + DT_NEEDED (for lazy PLT resolution).
    #[serde(default)]
    pub global_symbols: std::collections::HashMap<String, u64>,
}

const PT_INTERP: u32 = 3;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;
/// Preferred guest base for a PIE-style dynamic linker image.
const DEFAULT_INTERP_BASE: u64 = 0x0000_5555_5555_0000;
/// First guest base for DT_NEEDED shared libraries (grows upward).
const SHARED_LIB_BASE_START: u64 = 0x0000_7F00_0000_0000;
const LIB_BASE_STRIDE: u64 = 0x0000_0000_0200_0000; // 32 MiB slots

/// Magic PLT lazy-resolver entry (not a real GOT slot index).
/// First call through an unresolved PLT jumps here; the stub binds then tail-calls.
pub const PLT_RESOLVER_STUB: u64 = 0xFFFFFFF1_FFFF_FFF0;
/// Per-slot lazy marker base: GOT entries hold `PLT_LAZY_MARK | (index << 3)` until bound.
pub const PLT_LAZY_MARK: u64 = 0xFFFFFFF1_8000_0000;

/// True when lazy PLT binding is requested (`FISSION_LAZY_BIND=1`).
pub fn lazy_bind_enabled() -> bool {
    matches!(
        std::env::var("FISSION_LAZY_BIND").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Decode a lazy GOT marker into a table index.
pub fn lazy_mark_index(addr: u64) -> Option<usize> {
    if addr & 0xFFFF_FFFF_8000_0000 == PLT_LAZY_MARK {
        Some(((addr & 0x7FFF_FFF8) >> 3) as usize)
    } else if (PLT_LAZY_MARK..PLT_LAZY_MARK + 0x1000_0000).contains(&addr) {
        Some(((addr - PLT_LAZY_MARK) >> 3) as usize)
    } else {
        None
    }
}

pub fn make_lazy_mark(index: usize) -> u64 {
    PLT_LAZY_MARK | ((index as u64) << 3)
}

/// True when `val` looks like a mini-dynlink **shared-lib** binding we should keep.
///
/// Unresolved JUMP_SLOT entries in the ELF image usually still point into the
/// main binary's PLT stub (non-zero, non-magic). Those must **not** be treated
/// as resolved — HLE / lazy markers still need to overwrite them.
///
/// Successful SharedLibs resolves land at [`SHARED_LIB_BASE_START`] and above.
pub fn is_resolved_got_target(val: u64) -> bool {
    if val == 0 {
        return false;
    }
    if lazy_mark_index(val).is_some() {
        return false;
    }
    // Linux HLE trampolines live at/above 0xFFFFFFF0_0000_0000.
    if val >= 0xFFFFFFF0_0000_0000 {
        return false;
    }
    val >= SHARED_LIB_BASE_START
}

/// Runtime table for deferred PLT/GOT binding.
#[derive(Clone, Debug, Default)]
pub struct PltLazyTable {
    /// index → (GOT virtual address, symbol name)
    pub entries: Vec<(u64, String)>,
    /// Global symbol VA map accumulated from main + DT_NEEDED libs.
    pub globals: std::collections::HashMap<String, u64>,
}

impl PltLazyTable {
    /// Resolve `name` to a guest VA: table globals, extra globals, then HLE trampoline.
    pub fn resolve_target(
        &self,
        name: &str,
        hle_magic_base: u64,
        extra_globals: &std::collections::HashMap<String, u64>,
    ) -> u64 {
        if let Some(&va) = self.globals.get(name).or_else(|| extra_globals.get(name)) {
            return va;
        }
        if let Some((idx, _)) = self
            .entries
            .iter()
            .enumerate()
            .find(|(_, (_, n))| n == name)
        {
            return hle_magic_base + (idx as u64) * 8;
        }
        hle_magic_base
    }

    /// Bind slot `index`: write final target into GOT, return target VA.
    pub fn bind_slot(
        &self,
        state: &mut MachineState,
        index: usize,
        hle_magic_base: u64,
        extra_globals: &std::collections::HashMap<String, u64>,
    ) -> Option<u64> {
        let (got_va, name) = self.entries.get(index)?.clone();
        let target = self.resolve_target(&name, hle_magic_base, extra_globals);
        let _ = state.write_space(state.ram_space(), got_va, &target.to_le_bytes());
        tracing::info!(
            "plt lazy bind: [{}] {} @ GOT 0x{:X} -> 0x{:X}",
            index,
            name,
            got_va,
            target
        );
        Some(target)
    }
}

// DT_* tags (ELF)
const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_STRTAB: i64 = 5;
const DT_STRSZ: i64 = 10;
const DT_FLAGS: i64 = 30;
const DT_FLAGS_1: i64 = 0x6fff_fffb;
const DF_BIND_NOW: u64 = 0x8;
const DF_1_NOW: u64 = 0x1;
const STT_FUNC: u8 = 2;
const STT_OBJECT: u8 = 1;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
const SHN_UNDEF: u16 = 0;

/// Read PT_INTERP path from raw ELF bytes (64-bit LE).
pub fn parse_pt_interp(data: &[u8]) -> Option<String> {
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] {
        return None;
    }
    let is_64 = data[4] == 2;
    let is_le = data[5] == 1;
    if !is_64 || !is_le {
        return None;
    }
    let phoff = usize::try_from(u64::from_le_bytes(data[32..40].try_into().ok()?)).ok()?;
    let phentsize = usize::from(u16::from_le_bytes(data[54..56].try_into().ok()?));
    let phnum = usize::from(u16::from_le_bytes(data[56..58].try_into().ok()?));
    if phentsize < 56 {
        return None;
    }
    for i in 0..phnum {
        let off = phoff.checked_add(i.checked_mul(phentsize)?)?;
        let Some(end) = off.checked_add(56) else {
            return None;
        };
        if end > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().ok()?);
        if p_type != PT_INTERP {
            continue;
        }
        let p_offset =
            usize::try_from(u64::from_le_bytes(data[off + 8..off + 16].try_into().ok()?)).ok()?;
        let p_filesz = usize::try_from(u64::from_le_bytes(
            data[off + 32..off + 40].try_into().ok()?,
        ))
        .ok()?;
        let Some(file_end) = p_offset.checked_add(p_filesz) else {
            return None;
        };
        if p_offset == 0 || p_filesz == 0 || file_end > data.len() {
            return None;
        }
        let raw = data.get(p_offset..file_end)?;
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
pub fn prepare_dynlink(state: &mut MachineState, binary: &LoadedBinary) -> Result<DynlinkInfo> {
    let data = binary.inner().data.as_slice();
    let main_entry = binary.inner().entry_point;
    let interp = parse_pt_interp(data);
    let has_got = !binary.inner().iat_symbols.is_empty();
    let has_dynamic = elf64_has_pt_dynamic(data)?;

    if interp.is_none() && !has_got && !has_dynamic {
        return Ok(DynlinkInfo {
            mode: DynlinkMode::Static,
            main_entry,
            ..Default::default()
        });
    }

    let machine = elf64_machine(data)?;
    match machine {
        EM_X86_64 => prepare_dynlink_x86_64(state, binary, interp, has_got),
        EM_AARCH64 => prepare_dynlink_aarch64(state, binary, interp),
        other => anyhow::bail!(
            "unsupported ELF e_machine {other} in dynamic-link path (expected x86-64 ({EM_X86_64}) or AArch64 ({EM_AARCH64}))"
        ),
    }
}

fn prepare_dynlink_x86_64(
    state: &mut MachineState,
    binary: &LoadedBinary,
    guest_interp: Option<String>,
    has_got: bool,
) -> Result<DynlinkInfo> {
    let main_entry = binary.inner().entry_point;

    if dynlink_enabled() {
        if let Some(ref gpath) = guest_interp {
            if let Some(host) = host_interp_candidate(gpath) {
                match map_interpreter(state, &host, DEFAULT_INTERP_BASE, EM_X86_64) {
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
                            loaded_libs: Vec::new(),
                            bind_now: false,
                            global_symbols: std::collections::HashMap::new(),
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

    // Mini-dynlink: try DT_NEEDED shared lib load + BIND_NOW (no host ld.so required).
    // Even when we fall back to HleGot, keep collected globals for lazy PLT resolve.
    let mut hle_globals = std::collections::HashMap::new();
    if has_got || guest_interp.is_some() {
        match load_shared_libraries(state, binary) {
            Ok(shared) if !shared.loaded_libs.is_empty() || shared.bind_now_applied => {
                tracing::info!(
                    "dynlink: SharedLibs mode — {} libs, bind_now={}, globals={}",
                    shared.loaded_libs.len(),
                    shared.bind_now_applied,
                    shared.globals.len()
                );
                return Ok(DynlinkInfo {
                    mode: DynlinkMode::SharedLibs,
                    interp_path: guest_interp,
                    host_interp_path: None,
                    interp_base: 0,
                    interp_entry: 0,
                    main_entry,
                    loaded_libs: shared.loaded_libs,
                    // bind_now false when lazy mode forced even if DT flags say NOW
                    bind_now: shared.bind_now_applied && !lazy_bind_enabled(),
                    global_symbols: shared.globals,
                });
            }
            Ok(shared) => {
                hle_globals = shared.globals;
            }
            Err(e) => {
                tracing::debug!("dynlink: shared lib load skipped: {e:#}");
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
        loaded_libs: Vec::new(),
        bind_now: false,
        global_symbols: hle_globals,
    })
}

fn prepare_dynlink_aarch64(
    state: &mut MachineState,
    binary: &LoadedBinary,
    guest_interp: Option<String>,
) -> Result<DynlinkInfo> {
    if dynlink_enabled() && guest_interp.is_some() {
        anyhow::bail!(
            "AArch64 PT_INTERP execution is not supported; unset FISSION_ENABLE_DYNLINK to use the documented AArch64 HLE GOT path"
        );
    }

    // Image segments are mapped at their ELF virtual addresses, so this loader
    // currently has no load bias to add. Imports not defined by the main image
    // are left for LinuxEnv::patch_imports to bind to HLE trampolines.
    let globals = collect_global_symbols(binary.inner().data.as_slice(), 0);
    let stats = apply_rela_aarch64(state, binary.inner().data.as_slice(), 0, |name| {
        globals.get(name).copied()
    })?;
    tracing::info!(
        "AArch64 HLE dynlink: relative={} jump_slot={} glob_dat={} unresolved={}",
        stats.relative,
        stats.jump_slot,
        stats.glob_dat,
        stats.unresolved
    );

    Ok(DynlinkInfo {
        mode: DynlinkMode::HleGot,
        interp_path: guest_interp,
        host_interp_path: None,
        interp_base: 0,
        interp_entry: 0,
        main_entry: binary.inner().entry_point,
        loaded_libs: Vec::new(),
        bind_now: !lazy_bind_enabled(),
        global_symbols: globals,
    })
}

fn elf64_machine(data: &[u8]) -> Result<u16> {
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] {
        anyhow::bail!("dynamic-link input is not an ELF64 image");
    }
    if data[4] != 2 || data[5] != 1 {
        anyhow::bail!("dynamic-link path supports ELF64 little-endian images only");
    }
    Ok(u16::from_le_bytes([data[18], data[19]]))
}

fn elf64_has_pt_dynamic(data: &[u8]) -> Result<bool> {
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] || data[4] != 2 || data[5] != 1 {
        // ELF32 and non-ELF images have separate/no current dynamic-link path.
        return Ok(false);
    }
    let phoff = usize::try_from(u64::from_le_bytes(data[32..40].try_into().unwrap()))
        .context("program-header offset too large")?;
    let phentsize = usize::from(u16::from_le_bytes(data[54..56].try_into().unwrap()));
    let phnum = usize::from(u16::from_le_bytes(data[56..58].try_into().unwrap()));
    if phnum == 0 {
        return Ok(false);
    }
    if phentsize < 56 {
        anyhow::bail!("invalid ELF program-header size {phentsize}");
    }
    for i in 0..phnum {
        let off = phoff
            .checked_add(
                i.checked_mul(phentsize)
                    .context("program-header offset overflow")?,
            )
            .context("program-header offset overflow")?;
        let header_end = off
            .checked_add(4)
            .context("program-header bounds overflow")?;
        let header = data
            .get(off..header_end)
            .context("truncated ELF program header")?;
        if u32::from_le_bytes(header.try_into().unwrap()) == PT_DYNAMIC {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── DT_NEEDED shared library load loop ──────────────────────────────────────

struct SharedLoadResult {
    loaded_libs: Vec<(String, u64)>,
    bind_now_applied: bool,
    globals: std::collections::HashMap<String, u64>,
}

fn lib_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(p) = std::env::var("FISSION_LIB_PATH") {
        for part in p.split(':') {
            if !part.is_empty() {
                paths.push(PathBuf::from(part));
            }
        }
    }
    for p in [
        "/lib",
        "/lib64",
        "/usr/lib",
        "/usr/lib64",
        "/lib/x86_64-linux-gnu",
        "/usr/lib/x86_64-linux-gnu",
    ] {
        paths.push(PathBuf::from(p));
    }
    paths
}

fn find_library(soname: &str, search: &[PathBuf]) -> Option<PathBuf> {
    // Absolute soname
    let abs = PathBuf::from(soname);
    if abs.is_file() {
        return Some(abs);
    }
    for dir in search {
        let cand = dir.join(soname);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Parse DT_NEEDED sonames and bind-now flag from ELF dynamic section.
pub fn parse_dt_needed(data: &[u8]) -> (Vec<String>, bool) {
    let mut needed = Vec::new();
    let mut bind_now = false;
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] || data[4] != 2 || data[5] != 1 {
        return (needed, bind_now);
    }
    let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
    let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;

    let mut dyn_off = None;
    let mut dyn_filesz = 0usize;
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        if off + 56 > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        if p_type == PT_DYNAMIC {
            dyn_off =
                Some(u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap()) as usize);
            dyn_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().unwrap()) as usize;
            break;
        }
    }
    let Some(dyn_off) = dyn_off else {
        return (needed, bind_now);
    };

    // First pass: collect tags and find STRTAB via vaddr → file offset heuristic.
    let mut tags: Vec<(i64, u64)> = Vec::new();
    let mut i = 0;
    while dyn_off + i + 16 <= data.len() && i < dyn_filesz {
        let tag = i64::from_le_bytes(data[dyn_off + i..dyn_off + i + 8].try_into().unwrap());
        let val = u64::from_le_bytes(data[dyn_off + i + 8..dyn_off + i + 16].try_into().unwrap());
        if tag == DT_NULL {
            break;
        }
        tags.push((tag, val));
        i += 16;
    }

    let strtab_va = tags.iter().find(|(t, _)| *t == DT_STRTAB).map(|(_, v)| *v);
    let strsz = tags
        .iter()
        .find(|(t, _)| *t == DT_STRSZ)
        .map(|(_, v)| *v as usize)
        .unwrap_or(0);

    // Map STRTAB VA to file offset via PT_LOAD.
    let strtab_off = strtab_va.and_then(|va| vaddr_to_offset(data, va));

    for (tag, val) in &tags {
        if *tag == DT_FLAGS && (val & DF_BIND_NOW) != 0 {
            bind_now = true;
        }
        if *tag == DT_FLAGS_1 && (val & DF_1_NOW) != 0 {
            bind_now = true;
        }
        if *tag == DT_NEEDED {
            if let (Some(soff), true) = (strtab_off, strsz > 0) {
                let name_off = soff + *val as usize;
                if name_off < data.len() {
                    let end = data[name_off..]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|i| name_off + i)
                        .unwrap_or(data.len().min(name_off + 256));
                    let s = String::from_utf8_lossy(&data[name_off..end]).into_owned();
                    if !s.is_empty() {
                        needed.push(s);
                    }
                }
            }
        }
    }
    // Mini-dynlink always does eager resolve (BIND_NOW policy).
    let _ = bind_now;
    (needed, true)
}

fn vaddr_to_offset(data: &[u8], va: u64) -> Option<usize> {
    let phoff = u64::from_le_bytes(data[32..40].try_into().ok()?) as usize;
    let phentsize = u16::from_le_bytes(data[54..56].try_into().ok()?) as usize;
    let phnum = u16::from_le_bytes(data[56..58].try_into().ok()?) as usize;
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        if off + 56 > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().ok()?);
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().ok()?);
        let p_vaddr = u64::from_le_bytes(data[off + 16..off + 24].try_into().ok()?);
        let p_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().ok()?);
        if va >= p_vaddr && va < p_vaddr + p_filesz {
            return Some((p_offset + (va - p_vaddr)) as usize);
        }
    }
    // PIE often has vaddr == offset for early segments.
    if (va as usize) < data.len() {
        return Some(va as usize);
    }
    None
}

fn collect_global_symbols(data: &[u8], load_bias: u64) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if data.len() < 64 {
        return out;
    }
    let shoff = u64::from_le_bytes(data[40..48].try_into().unwrap()) as usize;
    let shentsize = u16::from_le_bytes(data[58..60].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(data[60..62].try_into().unwrap()) as usize;
    if shentsize < 64 || shoff == 0 {
        return out;
    }
    for si in 0..shnum {
        let soff = shoff + si * shentsize;
        if soff + 64 > data.len() {
            break;
        }
        let sh_type = u32::from_le_bytes(data[soff + 4..soff + 8].try_into().unwrap());
        // SHT_DYNSYM = 11
        if sh_type != 11 {
            continue;
        }
        let sym_off = u64::from_le_bytes(data[soff + 24..soff + 32].try_into().unwrap()) as usize;
        let sym_size = u64::from_le_bytes(data[soff + 32..soff + 40].try_into().unwrap()) as usize;
        let entsz = {
            let e = u64::from_le_bytes(data[soff + 56..soff + 64].try_into().unwrap()) as usize;
            if e > 0 { e } else { 24 }
        };
        let str_link = u32::from_le_bytes(data[soff + 40..soff + 44].try_into().unwrap()) as usize;
        if str_link >= shnum {
            continue;
        }
        let stro = shoff + str_link * shentsize;
        if stro + 64 > data.len() {
            continue;
        }
        let str_off = u64::from_le_bytes(data[stro + 24..stro + 32].try_into().unwrap()) as usize;
        let str_size = u64::from_le_bytes(data[stro + 32..stro + 40].try_into().unwrap()) as usize;
        let count = sym_size / entsz;
        for i in 0..count {
            let eoff = sym_off + i * entsz;
            if eoff + 24 > data.len() {
                break;
            }
            let st_name = u32::from_le_bytes(data[eoff..eoff + 4].try_into().unwrap()) as usize;
            let st_info = data[eoff + 4];
            let st_shndx = u16::from_le_bytes(data[eoff + 6..eoff + 8].try_into().unwrap());
            let st_value = u64::from_le_bytes(data[eoff + 8..eoff + 16].try_into().unwrap());
            if st_shndx == SHN_UNDEF || st_value == 0 {
                continue;
            }
            let bind = st_info >> 4;
            let ty = st_info & 0xf;
            if bind != STB_GLOBAL && bind != STB_WEAK {
                continue;
            }
            if ty != STT_FUNC && ty != STT_OBJECT && ty != 0 {
                continue;
            }
            if st_name >= str_size || str_off + st_name >= data.len() {
                continue;
            }
            let start = str_off + st_name;
            let end = data[start..]
                .iter()
                .position(|&b| b == 0)
                .map(|j| start + j)
                .unwrap_or(start);
            let name = String::from_utf8_lossy(&data[start..end]).into_owned();
            if name.is_empty() {
                continue;
            }
            out.entry(name)
                .or_insert(st_value.saturating_add(load_bias));
        }
    }
    out
}

/// Load DT_NEEDED libraries, collect globals, BIND_NOW-apply main RELA.
fn load_shared_libraries(
    state: &mut MachineState,
    binary: &LoadedBinary,
) -> Result<SharedLoadResult> {
    let mut staged_state = state.clone();
    let main_data = binary.inner().data.as_slice();
    let main_base = binary.inner().image_base;
    let (needed, bind_now) = parse_dt_needed(main_data);
    let search = lib_search_paths();

    let mut loaded_libs = Vec::new();
    let mut globals = collect_global_symbols(main_data, main_base);
    let mut next_base = SHARED_LIB_BASE_START;

    for soname in &needed {
        let Some(host) = find_library(soname, &search) else {
            tracing::debug!("dynlink: DT_NEEDED `{soname}` not found on host; skip");
            continue;
        };
        let mapped = map_interpreter(&mut staged_state, &host, next_base, EM_X86_64)
            .with_context(|| format!("map shared lib {}", host.display()))?;
        let lib_bytes =
            std::fs::read(&host).with_context(|| format!("read shared lib {}", host.display()))?;
        let lib_globals = collect_global_symbols(&lib_bytes, mapped.base);
        // Prefer first definition (main then earlier libs).
        for (k, v) in lib_globals {
            globals.entry(k).or_insert(v);
        }
        // Apply RELATIVE inside the library itself.
        let _ = apply_rela_x86_64(&mut staged_state, &lib_bytes, mapped.base, |name| {
            globals.get(name).copied()
        });
        loaded_libs.push((soname.clone(), mapped.base));
        next_base = next_base.saturating_add(LIB_BASE_STRIDE);
        tracing::info!(
            "dynlink: loaded `{}` from {} at base=0x{:X}",
            soname,
            host.display(),
            mapped.base
        );
    }

    let mut bind_now_applied = false;
    let eager = bind_now && !lazy_bind_enabled();
    if eager {
        // Eager BIND_NOW: RELATIVE + symbols found in loaded modules.
        // Unresolved JUMP_SLOT left for LinuxEnv::patch_imports HLE trampolines.
        let stats = apply_rela_x86_64(&mut staged_state, main_data, main_base, |name| {
            globals.get(name).copied()
        })?;
        bind_now_applied = stats.jump_slot > 0 || stats.relative > 0 || stats.glob_dat > 0;
    } else {
        // Lazy mode: still apply RELATIVE (base fixups), leave JUMP_SLOT for lazy PLT.
        let stats = apply_rela_x86_64(&mut staged_state, main_data, main_base, |_name| None)?;
        tracing::info!(
            "dynlink: lazy bind mode — applied {} RELATIVE, JUMP_SLOT deferred",
            stats.relative
        );
        // Also apply RELATIVE for each already-mapped lib (done above in loop).
        let _ = stats;
    }

    *state = staged_state;
    Ok(SharedLoadResult {
        loaded_libs,
        bind_now_applied,
        globals,
    })
}

/// Build a lazy PLT table from main binary `iat_symbols` + shared globals.
pub fn build_plt_lazy_table(
    binary: &LoadedBinary,
    globals: std::collections::HashMap<String, u64>,
) -> PltLazyTable {
    let mut entries: Vec<(u64, String)> = binary
        .inner()
        .iat_symbols
        .iter()
        .map(|(&addr, name)| {
            let bare = name
                .split('@')
                .next()
                .unwrap_or(name)
                .split('!')
                .next_back()
                .unwrap_or(name)
                .to_string();
            (addr, bare)
        })
        .collect();
    entries.sort_by_key(|(addr, _)| *addr);
    PltLazyTable { entries, globals }
}

/// Write lazy markers into GOT slots (in-memory). Call after sections are mapped.
pub fn install_lazy_got(state: &mut MachineState, table: &PltLazyTable) -> Result<()> {
    for (i, (got_va, name)) in table.entries.iter().enumerate() {
        let mark = make_lazy_mark(i);
        state.write_space(state.ram_space(), *got_va, &mark.to_le_bytes())?;
        tracing::debug!(
            "plt lazy install: [{}] {} GOT 0x{:X} mark=0x{:X}",
            i,
            name,
            got_va,
            mark
        );
    }
    Ok(())
}

#[derive(Debug)]
struct MappedInterp {
    base: u64,
    entry: u64,
}

/// Map a host ELF interpreter into guest RAM at `preferred_base` (PIE-friendly).
fn map_interpreter(
    state: &mut MachineState,
    path: &Path,
    preferred_base: u64,
    expected_machine: u16,
) -> Result<MappedInterp> {
    let interp_bin =
        LoadedBinary::from_file(path).with_context(|| format!("load interp {}", path.display()))?;
    let inner = interp_bin.inner();
    let data = inner.data.as_slice();
    let machine = elf64_machine(data).context("invalid ELF interpreter header")?;
    if machine != expected_machine {
        anyhow::bail!(
            "interpreter architecture mismatch: e_machine={machine}, expected {expected_machine}"
        );
    }
    let e_entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
    let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
    let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;
    if phentsize < 56 {
        anyhow::bail!("interpreter has invalid program-header size {phentsize}");
    }

    let mut headers = Vec::with_capacity(phnum);
    for i in 0..phnum {
        let off = phoff
            .checked_add(
                i.checked_mul(phentsize)
                    .context("program-header offset overflow")?,
            )
            .context("program-header offset overflow")?;
        let end = off
            .checked_add(56)
            .context("program-header bounds overflow")?;
        let ph = data
            .get(off..end)
            .context("truncated interpreter program header")?;
        headers.push((
            u32::from_le_bytes(ph[0..4].try_into().unwrap()),
            u32::from_le_bytes(ph[4..8].try_into().unwrap()),
            u64::from_le_bytes(ph[8..16].try_into().unwrap()),
            u64::from_le_bytes(ph[16..24].try_into().unwrap()),
            u64::from_le_bytes(ph[32..40].try_into().unwrap()),
            u64::from_le_bytes(ph[40..48].try_into().unwrap()),
        ));
    }

    // Prefer ELF preferred vaddrs; if PIE (min vaddr 0), place at preferred_base.
    let mut min_vaddr = u64::MAX;
    for &(p_type, _, _, p_vaddr, _, p_memsz) in &headers {
        if p_type != PT_LOAD {
            continue;
        }
        if p_memsz > 0 {
            min_vaddr = min_vaddr.min(p_vaddr);
        }
    }
    if min_vaddr == u64::MAX {
        anyhow::bail!("interpreter has no PT_LOAD");
    }
    let bias = if min_vaddr == 0 { preferred_base } else { 0 };

    let mut segments = Vec::new();
    for &(p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz) in &headers {
        if p_type != PT_LOAD {
            continue;
        }
        if p_memsz < p_filesz {
            anyhow::bail!("interpreter PT_LOAD has p_memsz smaller than p_filesz");
        }
        let p_offset = usize::try_from(p_offset).context("interpreter file offset too large")?;
        let p_filesz = usize::try_from(p_filesz).context("interpreter file size too large")?;
        let p_memsz_usize =
            usize::try_from(p_memsz).context("interpreter memory size too large")?;
        let file_end = p_offset
            .checked_add(p_filesz)
            .context("interpreter segment file range overflow")?;
        if file_end > data.len() {
            anyhow::bail!("interpreter PT_LOAD extends past end of file");
        }
        let guest_va = p_vaddr
            .checked_add(bias)
            .context("interpreter segment address overflow")?;
        guest_va
            .checked_add(p_memsz)
            .context("interpreter segment address overflow")?;
        let mut page_prot = prot::VALID | prot::READ;
        if p_flags & 2 != 0 {
            page_prot |= prot::WRITE;
        }
        if p_flags & 1 != 0 {
            page_prot |= prot::EXEC;
        }
        let mut buf = vec![0u8; p_memsz_usize];
        if p_filesz > 0 {
            buf[..p_filesz].copy_from_slice(&data[p_offset..file_end]);
        }
        segments.push((guest_va, p_memsz, page_prot, buf));
    }

    let mut staged = state.clone();
    for (guest_va, p_memsz, page_prot, buf) in segments {
        staged
            .write_space(state.ram_space(), guest_va, &buf)
            .with_context(|| format!("map interp segment at 0x{guest_va:X}"))?;
        staged
            .page_map
            .map_region(guest_va, p_memsz, page_prot, false);
    }
    *state = staged;

    Ok(MappedInterp {
        base: if bias != 0 { bias } else { min_vaddr },
        entry: e_entry.saturating_add(bias),
    })
}

// ── RELA application (mini dynamic linker for HLE / bootstrap) ─────────────

const R_X86_64_64: u32 = 1;
const R_X86_64_GLOB_DAT: u32 = 6;
const R_X86_64_JUMP_SLOT: u32 = 7;
const R_X86_64_RELATIVE: u32 = 8;
const R_AARCH64_ABS64: u32 = 257;
const R_AARCH64_GLOB_DAT: u32 = 1025;
const R_AARCH64_JUMP_SLOT: u32 = 1026;
const R_AARCH64_RELATIVE: u32 = 1027;
const SHT_RELA: u32 = 4;
const DT_PLTRELSZ: i64 = 2;
const DT_SYMTAB: i64 = 6;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const DT_SYMENT: i64 = 11;
const DT_REL: i64 = 17;
const DT_RELSZ: i64 = 18;
const DT_PLTREL: i64 = 20;
const DT_JMPREL: i64 = 23;
const SHN_ABS: u16 = 0xfff1;

/// Result of applying dynamic relocations to a guest image.
#[derive(Clone, Debug, Default)]
pub struct RelaApplyStats {
    pub relative: u64,
    pub absolute: u64,
    pub jump_slot: u64,
    pub glob_dat: u64,
    pub other: u64,
    pub unresolved: u64,
}

/// Apply SHT_RELA entries for an already-mapped ELF image in guest memory.
///
/// - `R_X86_64_RELATIVE`: `*slot = base + addend`
/// - `R_X86_64_JUMP_SLOT` / `GLOB_DAT`: write `resolve(name)` (typically HLE magic)
/// - Others: counted but left unchanged
///
/// This is the Fission mini-dynlink path used when full `ld.so` is unavailable.
pub fn apply_rela_x86_64(
    state: &mut MachineState,
    elf_bytes: &[u8],
    load_bias: u64,
    mut resolve: impl FnMut(&str) -> Option<u64>,
) -> Result<RelaApplyStats> {
    if elf_bytes.len() < 64 || elf_bytes[0..4] != [0x7f, b'E', b'L', b'F'] {
        anyhow::bail!("apply_rela: not ELF");
    }
    if elf_bytes[4] != 2 || elf_bytes[5] != 1 {
        anyhow::bail!("apply_rela: only ELF64 LE");
    }
    let machine = elf64_machine(elf_bytes)?;
    if machine != EM_X86_64 {
        anyhow::bail!("apply_rela_x86_64: e_machine={machine}, expected {EM_X86_64}");
    }
    let shoff = u64::from_le_bytes(elf_bytes[40..48].try_into().unwrap()) as usize;
    let shentsize = u16::from_le_bytes(elf_bytes[58..60].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(elf_bytes[60..62].try_into().unwrap()) as usize;
    if shentsize < 64 || shoff == 0 {
        return Ok(RelaApplyStats::default());
    }

    // Collect section headers lightly.
    let mut stats = RelaApplyStats::default();
    for si in 0..shnum {
        let soff = shoff + si * shentsize;
        if soff + 64 > elf_bytes.len() {
            break;
        }
        let sh_type = u32::from_le_bytes(elf_bytes[soff + 4..soff + 8].try_into().unwrap());
        if sh_type != SHT_RELA {
            continue;
        }
        let sh_offset =
            u64::from_le_bytes(elf_bytes[soff + 24..soff + 32].try_into().unwrap()) as usize;
        let sh_size =
            u64::from_le_bytes(elf_bytes[soff + 32..soff + 40].try_into().unwrap()) as usize;
        let sh_link =
            u32::from_le_bytes(elf_bytes[soff + 40..soff + 44].try_into().unwrap()) as usize;
        let sh_entsize =
            u64::from_le_bytes(elf_bytes[soff + 56..soff + 64].try_into().unwrap()) as usize;
        let entsz = if sh_entsize > 0 { sh_entsize } else { 24 };
        let count = sh_size / entsz;

        // Symbol table for name resolution.
        let (symtab, strtab) = symtab_strtab(elf_bytes, shoff, shentsize, shnum, sh_link);

        for ri in 0..count {
            let roff = sh_offset + ri * entsz;
            if roff + 24 > elf_bytes.len() {
                break;
            }
            let r_offset = u64::from_le_bytes(elf_bytes[roff..roff + 8].try_into().unwrap());
            let r_info = u64::from_le_bytes(elf_bytes[roff + 8..roff + 16].try_into().unwrap());
            let r_addend = i64::from_le_bytes(elf_bytes[roff + 16..roff + 24].try_into().unwrap());
            let r_type = (r_info & 0xffff_ffff) as u32;
            let r_sym = (r_info >> 32) as usize;
            // ET_EXEC/DYN: r_offset is VA (may already include image base).
            let slot = if r_offset >= load_bias {
                r_offset
            } else {
                r_offset.saturating_add(load_bias)
            };

            match r_type {
                R_X86_64_RELATIVE => {
                    let val = (load_bias as i64).wrapping_add(r_addend) as u64;
                    state.write_space(state.ram_space(), slot, &val.to_le_bytes())?;
                    stats.relative += 1;
                }
                R_X86_64_JUMP_SLOT | R_X86_64_GLOB_DAT | R_X86_64_64 => {
                    let name = sym_name(elf_bytes, symtab, strtab, r_sym).unwrap_or_default();
                    let bare = name.split('@').next().unwrap_or(&name);
                    if let Some(target) = resolve(bare) {
                        let val = target.wrapping_add(r_addend as u64);
                        state.write_space(state.ram_space(), slot, &val.to_le_bytes())?;
                        if r_type == R_X86_64_JUMP_SLOT {
                            stats.jump_slot += 1;
                        } else {
                            stats.glob_dat += 1;
                        }
                    } else {
                        stats.other += 1;
                    }
                }
                _ => {
                    stats.other += 1;
                }
            }
        }
    }
    tracing::info!(
        "apply_rela: relative={} jump_slot={} glob_dat={} other={}",
        stats.relative,
        stats.jump_slot,
        stats.glob_dat,
        stats.other
    );
    Ok(stats)
}

#[derive(Default)]
struct Aarch64DynamicTables {
    rela_ranges: Vec<(usize, usize)>,
    symtab_offset: Option<usize>,
    strtab: Option<(usize, usize)>,
    sym_entry_size: usize,
}

/// Apply the AArch64 dynamic relocation subset supported by the Linux HLE path.
///
/// `load_bias` is added to ELF virtual addresses. The current process-image
/// loader maps segments at their declared virtual addresses and passes zero.
/// All relocation records are decoded and checked before any guest bytes are
/// written, so an unsupported type or malformed symbol table leaves the image
/// untouched.
pub fn apply_rela_aarch64(
    state: &mut MachineState,
    elf_bytes: &[u8],
    load_bias: u64,
    mut resolve: impl FnMut(&str) -> Option<u64>,
) -> Result<RelaApplyStats> {
    let machine = elf64_machine(elf_bytes)?;
    if machine != EM_AARCH64 {
        anyhow::bail!("apply_rela_aarch64: e_machine={machine}, expected {EM_AARCH64}");
    }
    let tables = aarch64_dynamic_tables(elf_bytes)?;
    let mut stats = RelaApplyStats::default();
    let mut writes = Vec::new();

    for &(range_offset, range_size) in &tables.rela_ranges {
        let end = range_offset
            .checked_add(range_size)
            .context("AArch64 RELA range overflow")?;
        let bytes = elf_bytes
            .get(range_offset..end)
            .context("AArch64 RELA range is outside the ELF file")?;
        for relocation in bytes.chunks_exact(24) {
            let r_offset = u64::from_le_bytes(relocation[0..8].try_into().unwrap());
            let r_info = u64::from_le_bytes(relocation[8..16].try_into().unwrap());
            let r_addend = i64::from_le_bytes(relocation[16..24].try_into().unwrap());
            let r_type = (r_info & 0xffff_ffff) as u32;
            let r_sym = (r_info >> 32) as usize;

            if r_type == 0 {
                continue; // R_AARCH64_NONE
            }
            let slot = r_offset
                .checked_add(load_bias)
                .context("AArch64 relocation target address overflow")?;
            match r_type {
                R_AARCH64_RELATIVE => {
                    if r_sym != 0 {
                        anyhow::bail!(
                            "invalid R_AARCH64_RELATIVE at 0x{r_offset:X}: symbol index is {r_sym}"
                        );
                    }
                    writes.push((slot, load_bias.wrapping_add(r_addend as u64)));
                    stats.relative += 1;
                }
                R_AARCH64_ABS64 | R_AARCH64_GLOB_DAT | R_AARCH64_JUMP_SLOT => {
                    let (name, defined_value) =
                        aarch64_dynamic_symbol(elf_bytes, &tables, r_sym, load_bias)?;
                    let target = if !name.is_empty() {
                        resolve(&name).or(defined_value)
                    } else {
                        defined_value
                    };
                    if let Some(target) = target {
                        writes.push((slot, target.wrapping_add(r_addend as u64)));
                        match r_type {
                            R_AARCH64_ABS64 => stats.absolute += 1,
                            R_AARCH64_GLOB_DAT => stats.glob_dat += 1,
                            R_AARCH64_JUMP_SLOT => stats.jump_slot += 1,
                            _ => unreachable!(),
                        }
                    } else if r_type == R_AARCH64_JUMP_SLOT || r_type == R_AARCH64_GLOB_DAT {
                        if name.is_empty() {
                            anyhow::bail!(
                                "AArch64 relocation type {r_type} at 0x{r_offset:X} has no symbol name"
                            );
                        }
                        if r_addend != 0 {
                            anyhow::bail!(
                                "unresolved AArch64 import `{name}` at 0x{r_offset:X} has unsupported addend {r_addend}"
                            );
                        }
                        // LinuxEnv patches these imported slots to its HLE
                        // trampolines after the image and architecture exist.
                        stats.unresolved += 1;
                    } else {
                        anyhow::bail!(
                            "unresolved R_AARCH64_ABS64 symbol `{name}` at 0x{r_offset:X}"
                        );
                    }
                }
                other => anyhow::bail!(
                    "unsupported AArch64 ELF relocation type {other} at 0x{r_offset:X}"
                ),
            }
        }
    }

    // Commit only after every record is valid and every target write succeeds.
    let mut staged = state.clone();
    let ram = staged.ram_space();
    for (slot, value) in writes {
        staged.write_space(ram, slot, &value.to_le_bytes())?;
    }
    *state = staged;
    Ok(stats)
}

fn aarch64_dynamic_tables(data: &[u8]) -> Result<Aarch64DynamicTables> {
    let phoff = usize::try_from(u64::from_le_bytes(data[32..40].try_into().unwrap()))
        .context("program-header offset too large")?;
    let phentsize = usize::from(u16::from_le_bytes(data[54..56].try_into().unwrap()));
    let phnum = usize::from(u16::from_le_bytes(data[56..58].try_into().unwrap()));
    if phentsize < 56 {
        anyhow::bail!("invalid ELF program-header size {phentsize}");
    }

    let mut dynamic_range = None;
    for index in 0..phnum {
        let off = phoff
            .checked_add(
                index
                    .checked_mul(phentsize)
                    .context("program-header overflow")?,
            )
            .context("program-header overflow")?;
        let end = off
            .checked_add(56)
            .context("program-header bounds overflow")?;
        let ph = data.get(off..end).context("truncated ELF program header")?;
        if u32::from_le_bytes(ph[0..4].try_into().unwrap()) == PT_DYNAMIC {
            let file_offset = usize::try_from(u64::from_le_bytes(ph[8..16].try_into().unwrap()))
                .context("PT_DYNAMIC file offset too large")?;
            let file_size = usize::try_from(u64::from_le_bytes(ph[32..40].try_into().unwrap()))
                .context("PT_DYNAMIC file size too large")?;
            if dynamic_range.replace((file_offset, file_size)).is_some() {
                anyhow::bail!("ELF contains multiple PT_DYNAMIC segments");
            }
        }
    }

    let Some((dynamic_offset, dynamic_size)) = dynamic_range else {
        anyhow::bail!("AArch64 dynamic ELF has no PT_DYNAMIC segment");
    };
    if dynamic_size % 16 != 0 {
        anyhow::bail!("PT_DYNAMIC size is not a multiple of ELF64_Dyn");
    }
    let dynamic_end = dynamic_offset
        .checked_add(dynamic_size)
        .context("PT_DYNAMIC range overflow")?;
    let dynamic_bytes = data
        .get(dynamic_offset..dynamic_end)
        .context("PT_DYNAMIC range extends past end of ELF file")?;
    let mut tags = std::collections::HashMap::new();
    let mut terminated = false;
    for entry in dynamic_bytes.chunks_exact(16) {
        let tag = i64::from_le_bytes(entry[0..8].try_into().unwrap());
        let value = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        if tag == DT_NULL {
            terminated = true;
            break;
        }
        if matches!(
            tag,
            DT_NULL
                | DT_PLTRELSZ
                | DT_SYMTAB
                | DT_RELA
                | DT_RELASZ
                | DT_RELAENT
                | DT_STRSZ
                | DT_SYMENT
                | DT_REL
                | DT_RELSZ
                | DT_PLTREL
                | DT_JMPREL
                | DT_STRTAB
        ) {
            if let Some(old) = tags.insert(tag, value)
                && old != value
            {
                anyhow::bail!("ELF has conflicting dynamic tag values for {tag}");
            }
        } else {
            // Tags such as DT_NEEDED may legally repeat with different values.
            tags.entry(tag).or_insert(value);
        }
    }
    if !terminated {
        anyhow::bail!("PT_DYNAMIC has no DT_NULL terminator");
    }

    if tags.get(&DT_RELSZ).copied().unwrap_or(0) != 0 {
        anyhow::bail!("AArch64 DT_REL relocations are unsupported; expected RELA");
    }
    let mut ranges = Vec::new();
    let rela_size = tags.get(&DT_RELASZ).copied().unwrap_or(0);
    if rela_size > 0 {
        let rela_addr = *tags
            .get(&DT_RELA)
            .context("DT_RELASZ is present without DT_RELA")?;
        let rela_ent = tags.get(&DT_RELAENT).copied().unwrap_or(24);
        if rela_ent != 24 || rela_size % rela_ent != 0 {
            anyhow::bail!("unsupported AArch64 DT_RELA entry size {rela_ent}");
        }
        ranges.push((
            elf64_vaddr_to_offset(data, rela_addr, rela_size)?,
            usize::try_from(rela_size).context("DT_RELASZ too large")?,
        ));
    }

    let plt_size = tags.get(&DT_PLTRELSZ).copied().unwrap_or(0);
    if plt_size > 0 {
        let plt_addr = *tags
            .get(&DT_JMPREL)
            .context("DT_PLTRELSZ is present without DT_JMPREL")?;
        if tags.get(&DT_PLTREL).copied() != Some(DT_RELA as u64) {
            anyhow::bail!("AArch64 PLT relocations must use DT_RELA");
        }
        let rela_ent = tags.get(&DT_RELAENT).copied().unwrap_or(24);
        if rela_ent != 24 || plt_size % rela_ent != 0 {
            anyhow::bail!("unsupported AArch64 PLT RELA entry size {rela_ent}");
        }
        ranges.push((
            elf64_vaddr_to_offset(data, plt_addr, plt_size)?,
            usize::try_from(plt_size).context("DT_PLTRELSZ too large")?,
        ));
    }

    ranges.sort_unstable();
    ranges.dedup();
    for pair in ranges.windows(2) {
        let left_end = pair[0]
            .0
            .checked_add(pair[0].1)
            .context("AArch64 RELA range overflow")?;
        if left_end > pair[1].0 {
            anyhow::bail!("AArch64 dynamic relocation ranges overlap");
        }
    }

    let (symtab_offset, strtab, sym_entry_size) = if ranges.is_empty() {
        (None, None, 24)
    } else {
        let symtab_offset = tags
            .get(&DT_SYMTAB)
            .map(|&addr| elf64_vaddr_to_offset(data, addr, 24))
            .transpose()?;
        let strtab = match (tags.get(&DT_STRTAB), tags.get(&DT_STRSZ)) {
            (Some(&addr), Some(&size)) => Some((
                elf64_vaddr_to_offset(data, addr, size)?,
                usize::try_from(size).context("DT_STRSZ too large")?,
            )),
            (None, None) => None,
            _ => anyhow::bail!("incomplete AArch64 dynamic string-table metadata"),
        };
        let syment = tags.get(&DT_SYMENT).copied().unwrap_or(24);
        if syment != 24 {
            anyhow::bail!("unsupported AArch64 dynamic symbol size {syment}");
        }
        (
            symtab_offset,
            strtab,
            usize::try_from(syment).context("DT_SYMENT too large")?,
        )
    };

    Ok(Aarch64DynamicTables {
        rela_ranges: ranges,
        symtab_offset,
        strtab,
        sym_entry_size,
    })
}

fn elf64_vaddr_to_offset(data: &[u8], address: u64, size: u64) -> Result<usize> {
    let phoff = usize::try_from(u64::from_le_bytes(data[32..40].try_into().unwrap()))
        .context("program-header offset too large")?;
    let phentsize = usize::from(u16::from_le_bytes(data[54..56].try_into().unwrap()));
    let phnum = usize::from(u16::from_le_bytes(data[56..58].try_into().unwrap()));
    if phentsize < 56 {
        anyhow::bail!("invalid ELF program-header size {phentsize}");
    }
    let requested_end = address
        .checked_add(size)
        .context("ELF virtual range overflow")?;
    for index in 0..phnum {
        let off = phoff
            .checked_add(
                index
                    .checked_mul(phentsize)
                    .context("program-header overflow")?,
            )
            .context("program-header overflow")?;
        let end = off
            .checked_add(56)
            .context("program-header bounds overflow")?;
        let ph = data.get(off..end).context("truncated ELF program header")?;
        if u32::from_le_bytes(ph[0..4].try_into().unwrap()) != PT_LOAD {
            continue;
        }
        let file_offset = u64::from_le_bytes(ph[8..16].try_into().unwrap());
        let vaddr = u64::from_le_bytes(ph[16..24].try_into().unwrap());
        let filesz = u64::from_le_bytes(ph[32..40].try_into().unwrap());
        let segment_end = vaddr
            .checked_add(filesz)
            .context("PT_LOAD virtual range overflow")?;
        if address < vaddr || requested_end > segment_end {
            continue;
        }
        let offset = file_offset
            .checked_add(address - vaddr)
            .context("PT_LOAD file offset overflow")?;
        let offset = usize::try_from(offset).context("PT_LOAD file offset too large")?;
        let size = usize::try_from(size).context("ELF virtual range too large")?;
        offset
            .checked_add(size)
            .filter(|&end| end <= data.len())
            .context("ELF virtual range extends past end of file")?;
        return Ok(offset);
    }
    anyhow::bail!("ELF virtual address range 0x{address:X}..0x{requested_end:X} is not file-backed")
}

fn aarch64_dynamic_symbol(
    data: &[u8],
    tables: &Aarch64DynamicTables,
    index: usize,
    load_bias: u64,
) -> Result<(String, Option<u64>)> {
    if index == 0 {
        return Ok((String::new(), Some(0)));
    }
    let symtab = tables
        .symtab_offset
        .context("AArch64 relocation references a symbol but DT_SYMTAB is absent")?;
    let entry_offset = index
        .checked_mul(tables.sym_entry_size)
        .and_then(|delta| symtab.checked_add(delta))
        .context("AArch64 dynamic symbol offset overflow")?;
    let entry_end = entry_offset
        .checked_add(24)
        .context("AArch64 dynamic symbol range overflow")?;
    let entry = data
        .get(entry_offset..entry_end)
        .context("AArch64 dynamic symbol entry is outside the ELF file")?;
    let name_offset = u32::from_le_bytes(entry[0..4].try_into().unwrap()) as usize;
    let section_index = u16::from_le_bytes(entry[6..8].try_into().unwrap());
    let value = u64::from_le_bytes(entry[8..16].try_into().unwrap());
    let strtab = tables
        .strtab
        .context("AArch64 relocation references a symbol but DT_STRTAB is absent")?;
    if name_offset >= strtab.1 {
        anyhow::bail!("AArch64 dynamic symbol name offset is outside DT_STRTAB");
    }
    let name_start = strtab
        .0
        .checked_add(name_offset)
        .context("AArch64 dynamic symbol name offset overflow")?;
    let name_limit = strtab
        .0
        .checked_add(strtab.1)
        .context("AArch64 dynamic string-table range overflow")?;
    let name_bytes = data
        .get(name_start..name_limit)
        .context("AArch64 dynamic string table is outside the ELF file")?;
    let name_end = name_bytes
        .iter()
        .position(|&byte| byte == 0)
        .context("unterminated AArch64 dynamic symbol name")?;
    let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
    let defined_value = if section_index == 0 {
        None
    } else if section_index == SHN_ABS {
        Some(value)
    } else {
        Some(
            value
                .checked_add(load_bias)
                .context("AArch64 dynamic symbol value overflow")?,
        )
    };
    Ok((name, defined_value))
}

fn symtab_strtab(
    data: &[u8],
    shoff: usize,
    shentsize: usize,
    shnum: usize,
    sh_link: usize,
) -> (Option<(usize, usize, usize)>, Option<(usize, usize)>) {
    if sh_link >= shnum {
        return (None, None);
    }
    let soff = shoff + sh_link * shentsize;
    if soff + 64 > data.len() {
        return (None, None);
    }
    let sym_off = u64::from_le_bytes(data[soff + 24..soff + 32].try_into().unwrap()) as usize;
    let sym_size = u64::from_le_bytes(data[soff + 32..soff + 40].try_into().unwrap()) as usize;
    let sym_entsize = u64::from_le_bytes(data[soff + 56..soff + 64].try_into().unwrap()) as usize;
    let str_link = u32::from_le_bytes(data[soff + 40..soff + 44].try_into().unwrap()) as usize;
    if str_link >= shnum {
        return (Some((sym_off, sym_size, sym_entsize.max(24))), None);
    }
    let stroff_hdr = shoff + str_link * shentsize;
    if stroff_hdr + 64 > data.len() {
        return (Some((sym_off, sym_size, sym_entsize.max(24))), None);
    }
    let str_off =
        u64::from_le_bytes(data[stroff_hdr + 24..stroff_hdr + 32].try_into().unwrap()) as usize;
    let str_size =
        u64::from_le_bytes(data[stroff_hdr + 32..stroff_hdr + 40].try_into().unwrap()) as usize;
    (
        Some((
            sym_off,
            sym_size,
            if sym_entsize > 0 { sym_entsize } else { 24 },
        )),
        Some((str_off, str_size)),
    )
}

fn sym_name(
    data: &[u8],
    symtab: Option<(usize, usize, usize)>,
    strtab: Option<(usize, usize)>,
    index: usize,
) -> Option<String> {
    let (sym_off, sym_size, entsz) = symtab?;
    let (str_off, str_size) = strtab?;
    let eoff = sym_off + index * entsz;
    if eoff + 4 > data.len() || eoff + entsz > sym_off + sym_size {
        return None;
    }
    let st_name = u32::from_le_bytes(data[eoff..eoff + 4].try_into().ok()?) as usize;
    if st_name >= str_size || str_off + st_name >= data.len() {
        return None;
    }
    let start = str_off + st_name;
    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|i| start + i)
        .unwrap_or(data.len().min(start + 256));
    Some(String::from_utf8_lossy(&data[start..end]).into_owned())
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

    #[test]
    fn apply_rela_writes_jump_slots_for_dyn_puts() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf");
        if !path.is_file() {
            return;
        }
        let binary = LoadedBinary::from_file(&path).unwrap();
        let mut state = MachineState::new();
        // Map sections like the image loader would.
        let info = crate::os::linux::image_info::load_elf_image(
            &mut state,
            &binary,
            &crate::os::linux::image_info::ProcessArgs::default(),
        )
        .unwrap();
        let data = binary.inner().data.as_slice();
        let mut resolved = 0u64;
        let stats = apply_rela_x86_64(&mut state, data, info.load_addr, |name| {
            if name == "puts" || name == "__libc_start_main" {
                resolved += 1;
                Some(0xFFFFFFF1_00000000 + resolved * 8)
            } else {
                Some(0)
            }
        })
        .expect("apply_rela");
        assert!(
            stats.jump_slot >= 1 || stats.glob_dat >= 1,
            "expected JUMP_SLOT/GLOB_DAT applies: {stats:?}"
        );
    }

    #[test]
    fn parse_dt_needed_from_dyn_puts() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf");
        if !path.is_file() {
            return;
        }
        let data = std::fs::read(&path).unwrap();
        let (needed, bind_now) = parse_dt_needed(&data);
        // musl dynamic hello typically needs libc.so
        assert!(
            needed
                .iter()
                .any(|n| n.contains("libc") || n.contains("ld-")),
            "expected libc/ld in DT_NEEDED, got {needed:?}"
        );
        assert!(bind_now, "mini-dynlink defaults BIND_NOW");
    }

    #[test]
    fn lazy_mark_roundtrip() {
        let m = make_lazy_mark(3);
        assert_eq!(lazy_mark_index(m), Some(3));
        assert!(lazy_mark_index(0x400000).is_none());
    }

    #[test]
    fn plt_lazy_bind_writes_got() {
        let mut state = MachineState::new();
        let got = 0x1000u64;
        state.page_map.map_region(got, 0x1000, prot::RW, true);
        state
            .write_space(state.ram_space(), got, &make_lazy_mark(0).to_le_bytes())
            .unwrap();
        let mut table = PltLazyTable::default();
        table.entries.push((got, "puts".into()));
        table.globals.insert("puts".into(), 0x401000);
        let empty = std::collections::HashMap::new();
        let t = table
            .bind_slot(&mut state, 0, 0xFFFFFFF100000000, &empty)
            .unwrap();
        assert_eq!(t, 0x401000);
        let bytes = state.read_space(state.ram_space(), got, 8).unwrap();
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), 0x401000);
    }

    #[test]
    fn is_resolved_got_target_filters_magic_and_plt_stubs() {
        // Main-image PLT stubs must NOT count as resolved.
        assert!(!is_resolved_got_target(0x401000));
        assert!(!is_resolved_got_target(0));
        assert!(!is_resolved_got_target(0xFFFFFFF1_0000_0000));
        assert!(!is_resolved_got_target(make_lazy_mark(2)));
        // Shared-lib slot range used by mini-dynlink.
        assert!(is_resolved_got_target(SHARED_LIB_BASE_START + 0x1234));
    }

    /// After full image load, RELA JUMP_SLOT writes must stick (map → rela order).
    #[test]
    fn load_elf_then_rela_jump_slot_persists() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf");
        if !path.is_file() {
            return;
        }
        let binary = LoadedBinary::from_file(&path).unwrap();
        let mut state = MachineState::new();
        let info = crate::os::linux::image_info::load_elf_image(
            &mut state,
            &binary,
            &crate::os::linux::image_info::ProcessArgs::default(),
        )
        .unwrap();
        let data = binary.inner().data.as_slice();
        let sentinel = 0x0000_0000_0042_4242u64;
        let stats = apply_rela_x86_64(&mut state, data, info.load_addr, |name| {
            if name == "puts" || name == "__libc_start_main" {
                Some(sentinel)
            } else {
                None
            }
        })
        .expect("apply_rela");
        assert!(
            stats.jump_slot >= 1 || stats.glob_dat >= 1,
            "expected JUMP_SLOT/GLOB_DAT: {stats:?}"
        );
        // At least one iat slot should now hold the sentinel.
        let mut found = false;
        for &addr in binary.inner().iat_symbols.keys() {
            if let Ok(bytes) = state.read_space(state.ram_space(), addr, 8) {
                let v = u64::from_le_bytes(bytes.try_into().unwrap());
                if v == sentinel {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "RELA sentinel wiped or not applied to any GOT slot");
    }

    fn aarch64_dyn_fixture() -> (PathBuf, Vec<u8>) {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/aarch64_dyn_import.elf");
        let bytes = std::fs::read(&path).expect("read checked-in AArch64 dynamic fixture");
        (path, bytes)
    }

    fn relocation_entries(data: &[u8]) -> Vec<(u64, u32, i64, usize)> {
        let tables = aarch64_dynamic_tables(data).expect("parse AArch64 dynamic tables");
        tables
            .rela_ranges
            .iter()
            .flat_map(|&(offset, size)| {
                data[offset..offset + size]
                    .chunks_exact(24)
                    .enumerate()
                    .map(move |(index, relocation)| {
                        (
                            u64::from_le_bytes(relocation[0..8].try_into().unwrap()),
                            (u64::from_le_bytes(relocation[8..16].try_into().unwrap())
                                & 0xffff_ffff) as u32,
                            i64::from_le_bytes(relocation[16..24].try_into().unwrap()),
                            offset + index * 24,
                        )
                    })
            })
            .collect()
    }

    #[test]
    fn aarch64_dynamic_fixture_applies_relative_and_import_relocations() {
        let (path, data) = aarch64_dyn_fixture();
        assert_eq!(elf64_machine(&data).unwrap(), EM_AARCH64);
        let binary = LoadedBinary::from_file(&path).expect("load AArch64 fixture");
        assert!(
            binary
                .inner()
                .iat_symbols
                .values()
                .any(|name| name == "puts")
        );

        let mut state = MachineState::new();
        let image = crate::os::linux::image_info::load_elf_image(
            &mut state,
            &binary,
            &crate::os::linux::image_info::ProcessArgs::default(),
        )
        .expect("map AArch64 image and apply HLE relocations");
        assert_eq!(image.dynlink.mode, DynlinkMode::HleGot);
        assert!(image.dynlink.loaded_libs.is_empty());

        let entries = relocation_entries(&data);
        let (relative_slot, relative_addend) = entries
            .iter()
            .find(|(_, kind, _, _)| *kind == R_AARCH64_RELATIVE)
            .map(|(slot, _, addend, _)| (*slot, *addend))
            .expect("fixture contains R_AARCH64_RELATIVE");
        let relocated = state
            .read_space(state.ram_space(), relative_slot, 8)
            .unwrap();
        assert_eq!(
            u64::from_le_bytes(relocated.try_into().unwrap()),
            relative_addend as u64
        );

        let sentinel = 0x0000_0000_0042_4242u64;
        let stats = apply_rela_aarch64(&mut state, &data, 0, |name| {
            (name == "puts").then_some(sentinel)
        })
        .expect("apply supported AArch64 relocations");
        assert_eq!(stats.relative, 1);
        assert_eq!(stats.jump_slot, 1);
        assert_eq!(stats.unresolved, 0);
        let jump_slot = entries
            .iter()
            .find(|(_, kind, _, _)| *kind == R_AARCH64_JUMP_SLOT)
            .map(|(slot, _, _, _)| *slot)
            .expect("fixture contains R_AARCH64_JUMP_SLOT");
        let imported = state.read_space(state.ram_space(), jump_slot, 8).unwrap();
        assert_eq!(u64::from_le_bytes(imported.try_into().unwrap()), sentinel);
    }

    #[test]
    fn unsupported_aarch64_relocation_leaves_prior_slots_unchanged() {
        let (_, mut data) = aarch64_dyn_fixture();
        let entries = relocation_entries(&data);
        let relative_slot = entries
            .iter()
            .find(|(_, kind, _, _)| *kind == R_AARCH64_RELATIVE)
            .map(|(slot, _, _, _)| *slot)
            .unwrap();
        let jump = entries
            .iter()
            .find(|(_, kind, _, _)| *kind == R_AARCH64_JUMP_SLOT)
            .unwrap();
        let jump_slot = jump.0;
        let jump_info_offset = jump.3 + 8;
        let old_info = u64::from_le_bytes(
            data[jump_info_offset..jump_info_offset + 8]
                .try_into()
                .unwrap(),
        );
        let unknown_info = (old_info & !0xffff_ffff) | 0x7fff;
        data[jump_info_offset..jump_info_offset + 8].copy_from_slice(&unknown_info.to_le_bytes());

        let mut state = MachineState::new();
        state
            .page_map
            .map_region(0x20_000, 0x12_000, prot::RW, true);
        let before_relative = 0x1111_2222_3333_4444u64;
        let before_jump = 0x5555_6666_7777_8888u64;
        state
            .write_space(
                state.ram_space(),
                relative_slot,
                &before_relative.to_le_bytes(),
            )
            .unwrap();
        state
            .write_space(state.ram_space(), jump_slot, &before_jump.to_le_bytes())
            .unwrap();

        let error = apply_rela_aarch64(&mut state, &data, 0, |_| None)
            .expect_err("unknown relocation must fail");
        assert!(format!("{error:#}").contains("unsupported AArch64 ELF relocation type"));
        let relative = state
            .read_space(state.ram_space(), relative_slot, 8)
            .unwrap();
        let jump = state.read_space(state.ram_space(), jump_slot, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(relative.try_into().unwrap()),
            before_relative
        );
        assert_eq!(u64::from_le_bytes(jump.try_into().unwrap()), before_jump);
    }

    #[test]
    fn interpreter_architecture_is_checked_before_mapping_segments() {
        let (path, _) = aarch64_dyn_fixture();
        let mut state = MachineState::new();
        let error = map_interpreter(&mut state, &path, DEFAULT_INTERP_BASE, EM_X86_64)
            .expect_err("AArch64 image cannot be mapped as an x86-64 interpreter");
        assert!(format!("{error:#}").contains("interpreter architecture mismatch"));
        assert!(state.page_map.mappings().is_empty());
    }

    #[test]
    fn x86_relocator_rejects_aarch64_elf_before_writing() {
        let (_, data) = aarch64_dyn_fixture();
        let mut state = MachineState::new();
        let error = apply_rela_x86_64(&mut state, &data, 0, |_| Some(0x1234))
            .expect_err("x86-64 relocation path must reject AArch64 ELF");
        assert!(format!("{error:#}").contains("expected 62"));
        assert!(state.page_map.mappings().is_empty());
    }
}
