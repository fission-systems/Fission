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
            Ok(shared)
                if !shared.loaded_libs.is_empty() || shared.bind_now_applied =>
            {
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
            dyn_off = Some(u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap()) as usize);
            dyn_filesz =
                u64::from_le_bytes(data[off + 32..off + 40].try_into().unwrap()) as usize;
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

    let strtab_va = tags
        .iter()
        .find(|(t, _)| *t == DT_STRTAB)
        .map(|(_, v)| *v);
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
            if e > 0 {
                e
            } else {
                24
            }
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
            out.entry(name).or_insert(st_value.saturating_add(load_bias));
        }
    }
    out
}

/// Load DT_NEEDED libraries, collect globals, BIND_NOW-apply main RELA.
fn load_shared_libraries(
    state: &mut MachineState,
    binary: &LoadedBinary,
) -> Result<SharedLoadResult> {
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
        let mapped = map_interpreter(state, &host, next_base)
            .with_context(|| format!("map shared lib {}", host.display()))?;
        let lib_bytes = std::fs::read(&host)
            .with_context(|| format!("read shared lib {}", host.display()))?;
        let lib_globals = collect_global_symbols(&lib_bytes, mapped.base);
        // Prefer first definition (main then earlier libs).
        for (k, v) in lib_globals {
            globals.entry(k).or_insert(v);
        }
        // Apply RELATIVE inside the library itself.
        let _ = apply_rela_x86_64(state, &lib_bytes, mapped.base, |name| {
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
        let stats = apply_rela_x86_64(state, main_data, main_base, |name| {
            globals.get(name).copied()
        })?;
        bind_now_applied = stats.jump_slot > 0 || stats.relative > 0 || stats.glob_dat > 0;
    } else {
        // Lazy mode: still apply RELATIVE (base fixups), leave JUMP_SLOT for lazy PLT.
        let stats = apply_rela_x86_64(state, main_data, main_base, |_name| None)?;
        tracing::info!(
            "dynlink: lazy bind mode — applied {} RELATIVE, JUMP_SLOT deferred",
            stats.relative
        );
        // Also apply RELATIVE for each already-mapped lib (done above in loop).
        let _ = stats;
    }

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
                .last()
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
        tracing::debug!("plt lazy install: [{}] {} GOT 0x{:X} mark=0x{:X}", i, name, got_va, mark);
    }
    Ok(())
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

// ── RELA application (mini dynamic linker for HLE / bootstrap) ─────────────

const R_X86_64_64: u32 = 1;
const R_X86_64_GLOB_DAT: u32 = 6;
const R_X86_64_JUMP_SLOT: u32 = 7;
const R_X86_64_RELATIVE: u32 = 8;
const SHT_RELA: u32 = 4;

/// Result of applying dynamic relocations to a guest image.
#[derive(Clone, Debug, Default)]
pub struct RelaApplyStats {
    pub relative: u64,
    pub jump_slot: u64,
    pub glob_dat: u64,
    pub other: u64,
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
        let sh_offset = u64::from_le_bytes(elf_bytes[soff + 24..soff + 32].try_into().unwrap()) as usize;
        let sh_size = u64::from_le_bytes(elf_bytes[soff + 32..soff + 40].try_into().unwrap()) as usize;
        let sh_link = u32::from_le_bytes(elf_bytes[soff + 40..soff + 44].try_into().unwrap()) as usize;
        let sh_entsize = u64::from_le_bytes(elf_bytes[soff + 56..soff + 64].try_into().unwrap()) as usize;
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
    let sym_entsize =
        u64::from_le_bytes(data[soff + 56..soff + 64].try_into().unwrap()) as usize;
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
        Some((sym_off, sym_size, if sym_entsize > 0 { sym_entsize } else { 24 })),
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
            needed.iter().any(|n| n.contains("libc") || n.contains("ld-")),
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
        state
            .page_map
            .map_region(got, 0x1000, prot::RW, true);
        state
            .write_space(state.ram_space(), got, &make_lazy_mark(0).to_le_bytes())
            .unwrap();
        let mut table = PltLazyTable::default();
        table.entries.push((got, "puts".into()));
        table.globals.insert("puts".into(), 0x401000);
        let empty = std::collections::HashMap::new();
        let t = table.bind_slot(&mut state, 0, 0xFFFFFFF100000000, &empty).unwrap();
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
}
