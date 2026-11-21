//! Linux syscall HLE (user-mode).
//!
//! Coverage is intentionally pragmatic: enough for static musl/glibc startup
//! stubs and simple utilities. Expand by adding `SimProcedure`s and registering
//! numbers in `LinuxEnv::new`. QEMU linux-user is a structural reference only.
//!
//! Handlers reach arguments through [`Emulator::syscall_arg`] and answer
//! through [`Emulator::set_syscall_return`], never by register name. They used
//! to name `RDI`/`RSI`/`RDX`/`RAX` directly, which made every one of them
//! x86-64-only -- an aarch64 image could not make a single syscall no matter
//! what else worked. See [`super::syscall_conv`] for which registers each
//! architecture actually uses, and for the fact that the numbering differs too.

use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::linux::abi::{StatLayout, TargetStat};
use crate::os::linux::libc::read_string;
use crate::os::procedure::SimProcedure;
use crate::pcode::page_map::{page_align_up, prot};
use anyhow::Result;

fn ram_write(emu: &mut Emulator, addr: u64, data: &[u8]) -> Result<()> {
    let ram = emu.state.ram_space();
    emu.state.write_space(ram, addr, data)
}

/// Lay `stat` out the way the `stat`-family syscall in progress reports it
/// on this guest, and store it.
fn write_stat(emu: &mut Emulator, statbuf: u64, stat: &TargetStat) -> Result<()> {
    let number = emu.raw_syscall_number();
    let layout = StatLayout::for_syscall(&emu.arch, number);
    ram_write(emu, statbuf, &stat.to_bytes(layout))
}

/// A pointer-sized little-endian word of guest memory, or 0 where it cannot
/// be read.
fn read_word(emu: &mut Emulator, addr: u64) -> u64 {
    let width = usize::from(emu.arch.pointer_size).min(8);
    let mut raw = [0u8; 8];
    if let Ok(bytes) = ram_read(emu, addr, width) {
        raw[..width].copy_from_slice(&bytes);
    }
    u64::from_le_bytes(raw)
}

fn ram_read(emu: &mut Emulator, addr: u64, size: usize) -> Result<Vec<u8>> {
    let ram = emu.state.ram_space();
    emu.state.read_space(ram, addr, size)
}

/// The most one `read` or `write` transfers, as on Linux (`MAX_RW_COUNT`): a
/// larger count is not an error, it is simply not all honoured.
const MAX_RW_COUNT: u64 = 0x7FFF_F000;

/// How much of a guest buffer is held on the host at a time while it is
/// being transferred.
const IO_CHUNK: usize = 1 << 20;

const EFAULT: i64 = 14;

/// `write(fd, buf, count)` for whatever of `buf` the guest can read.
///
/// The count is the guest's. Linux transfers what is readable from `buf` on
/// and stops at the first page that is not -- `EFAULT` only when that is the
/// very first -- so a count of `(size_t)-1` over a five-byte buffer writes
/// five bytes. Reading `count` bytes into one host buffer first is neither
/// that nor safe.
pub(crate) fn write_from_guest(emu: &mut Emulator, fd: u64, buf: u64, count: u64) -> i64 {
    let count = count.min(MAX_RW_COUNT) as usize;
    let readable = emu.state.readable_len(buf, count);
    if count > 0 && readable == 0 {
        return -EFAULT;
    }
    let mut written = 0usize;
    while written < readable {
        let want = (readable - written).min(IO_CHUNK);
        let Ok(data) = ram_read(emu, buf.wrapping_add(written as u64), want) else {
            break;
        };
        match emu.guest_write(fd, &data) {
            Ok(n) => {
                written += n;
                if n < want {
                    break;
                }
            }
            // Nothing transferred yet: the error is the answer. Otherwise the
            // bytes that went are, and the error is the next call's.
            Err(error) if written == 0 => return -error.errno(),
            Err(_) => break,
        }
    }
    written as i64
}

// ── File I/O ─────────────────────────────────────────────────────────────────

pub struct SysRead;
impl SimProcedure for SysRead {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0);
        let buf = emu.syscall_arg(1);
        let count = emu.syscall_arg(2);

        tracing::info!("sys_read({}, 0x{:X}, {})", fd, buf, count);

        match emu.vfs.read(fd, count as usize) {
            Ok(data) => {
                let bytes_read = data.len();
                if bytes_read > 0 {
                    ram_write(emu, buf, &data)?;
                }
                if fd == 0 {
                    for i in 0..bytes_read {
                        let node = emu.solver.register_var(
                            format!("stdin_{}", buf + (i as u64)),
                            crate::bit_width_from_byte_size(1),
                        );
                        emu.state
                            .set_shadow_memory(emu.state.ram_space(), buf + (i as u64), node);
                    }
                }
                emu.set_syscall_return(bytes_read as u64)?;
            }
            Err(_) => {
                emu.set_syscall_return((-1i64) as u64)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysWrite;
impl SimProcedure for SysWrite {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0);
        let buf = emu.syscall_arg(1);
        let count = emu.syscall_arg(2);

        tracing::info!("sys_write({}, 0x{:X}, {})", fd, buf, count);

        let result = write_from_guest(emu, fd, buf, count);
        emu.set_syscall_return(result as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysOpen;
impl SimProcedure for SysOpen {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let filename_ptr = emu.syscall_arg(0);
        let filename = read_string(emu, filename_ptr).unwrap_or_else(|_| "unknown".into());
        tracing::info!("sys_open(\"{}\")", filename);
        let fd = emu.vfs.open(&filename, Vec::new());
        emu.set_syscall_return(fd)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysOpenat;
impl SimProcedure for SysOpenat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // openat(dirfd, pathname, flags, mode) — x86-64: rdi, rsi, rdx, r10
        let _dirfd = emu.syscall_arg(0) as i64;
        let pathname = emu.syscall_arg(1);
        let flags = emu.syscall_arg(2);
        let filename = read_string(emu, pathname).unwrap_or_else(|_| "unknown".into());
        tracing::info!(
            "sys_openat(dirfd={}, \"{}\", flags=0x{:X})",
            _dirfd,
            filename,
            flags
        );
        // O_DIRECTORY / missing file: still return a seed fd when path is registered.
        let fd = emu.vfs.open(&filename, Vec::new());
        // If open produced an empty never-seeded file and path is absolute missing, return -ENOENT.
        if let Some(sz) = emu.vfs.file_size(fd) {
            if sz == 0
                && !emu.vfs.path_seeds.contains_key(&filename)
                && !std::path::Path::new(&filename).is_file()
                && !filename.is_empty()
                && filename.starts_with('/')
            {
                let _ = emu.vfs.close(fd);
                // ENOENT = 2
                emu.set_syscall_return((-2i64) as u64)?;
                return Ok(HleResult::Continue);
            }
        }
        emu.set_syscall_return(fd)?;
        Ok(HleResult::Continue)
    }
}

/// Read only symlinks explicitly present in the guest VFS; never follow a host path.
pub struct SysReadlinkat;
impl SimProcedure for SysReadlinkat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let _dirfd = emu.syscall_arg(0) as i64;
        let path_ptr = emu.syscall_arg(1);
        let buf = emu.syscall_arg(2);
        let bufsiz = emu.syscall_arg(3) as usize;
        let path = match read_string(emu, path_ptr) {
            Ok(path) => path,
            Err(_) => {
                emu.set_syscall_return((-14i64) as u64)?; // EFAULT
                return Ok(HleResult::Continue);
            }
        };
        let Some(target) = emu.vfs.readlink(&path) else {
            emu.set_syscall_return((-2i64) as u64)?; // ENOENT
            return Ok(HleResult::Continue);
        };
        let bytes = target.as_bytes();
        let count = bytes.len().min(bufsiz);
        if count > 0 {
            ram_write(emu, buf, &bytes[..count])?;
        }
        emu.set_syscall_return(count as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysClose;
impl SimProcedure for SysClose {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0);
        tracing::info!("sys_close({})", fd);
        if emu.vfs.close(fd).is_ok() {
            emu.set_syscall_return(0)?;
        } else {
            emu.set_syscall_return((-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysFstat;
impl SimProcedure for SysFstat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0);
        let statbuf = emu.syscall_arg(1);
        tracing::info!("sys_fstat({}, 0x{:X})", fd, statbuf);

        if let Some(file) = emu.vfs.files.get(&fd) {
            let mut target_st = TargetStat::default();
            target_st.st_size = file.content.len() as i64;
            target_st.st_mode = 0x81B4;
            write_stat(emu, statbuf, &target_st)?;
            emu.set_syscall_return(0)?;
        } else if fd <= 2 {
            let mut target_st = TargetStat::default();
            target_st.st_mode = 0x2180; // char device-ish for stdio
            write_stat(emu, statbuf, &target_st)?;
            emu.set_syscall_return(0)?;
        } else {
            emu.set_syscall_return((-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysNewfstatat;
impl SimProcedure for SysNewfstatat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // newfstatat(dirfd, pathname, statbuf, flags)
        let pathname = emu.syscall_arg(1);
        let statbuf = emu.syscall_arg(2);
        let name = read_string(emu, pathname).unwrap_or_default();
        tracing::info!("sys_newfstatat(\"{}\")", name);
        let mut target_st = TargetStat::default();
        target_st.st_mode = 0x81B4;
        target_st.st_nlink = 1;
        target_st.st_blksize = 4096;
        if let Some(content) = emu.vfs.path_seeds.get(&name).cloned().or_else(|| {
            std::path::Path::new(&name)
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|b| emu.vfs.path_seeds.get(b).cloned())
        }) {
            target_st.st_size = content.len() as i64;
            target_st.st_blocks = (content.len() as i64 + 511) / 512;
        }
        write_stat(emu, statbuf, &target_st)?;
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

/// Where an `lseek` lands. `offset` is a signed `off_t`: a negative `SEEK_CUR`
/// or `SEEK_END` offset moves backwards, and a result before the start of the
/// file (or an unknown `whence`) is an error. Read as an unsigned `usize`, `-1`
/// was `usize::MAX` and sent the cursor to the end of the address space.
pub(crate) fn lseek_target(cursor: usize, len: usize, offset: i64, whence: u64) -> Option<usize> {
    let base: i128 = match whence {
        0 => 0,              // SEEK_SET
        1 => cursor as i128, // SEEK_CUR
        2 => len as i128,    // SEEK_END
        _ => return None,
    };
    usize::try_from(base + i128::from(offset)).ok()
}

pub struct SysLseek;
impl SimProcedure for SysLseek {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0);
        let offset = emu.syscall_arg(1) as i64;
        let whence = emu.syscall_arg(2);
        tracing::info!("sys_lseek({}, {}, {})", fd, offset, whence);
        if let Some(file) = emu.vfs.files.get_mut(&fd) {
            let Some(new) = lseek_target(file.cursor, file.content.len(), offset, whence) else {
                emu.set_syscall_return((-1i64) as u64)?;
                return Ok(HleResult::Continue);
            };
            file.seek(new);
            emu.set_syscall_return(new as u64)?;
        } else {
            emu.set_syscall_return((-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysWritev;
impl SimProcedure for SysWritev {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // writev(fd, iov, iovcnt). `struct iovec` is two pointer-sized words,
        // `{ void *base; size_t len; }`: 16 bytes on a 64-bit guest and 8 on
        // a 32-bit one. It was read as 16 everywhere, which on a 32-bit guest
        // turns each pair of entries into one whose pointer is a pointer and
        // a length glued together.
        let fd = emu.syscall_arg(0);
        let iov = emu.syscall_arg(1);
        let iovcnt = emu.syscall_arg(2);
        // Linux: more than IOV_MAX entries is EINVAL.
        const IOV_MAX: u64 = 1024;
        if iovcnt > IOV_MAX {
            emu.set_syscall_return((-22i64) as u64)?;
            return Ok(HleResult::Continue);
        }
        let word = u64::from(emu.arch.pointer_size);
        let mut total = 0i64;
        for i in 0..iovcnt {
            let entry = iov.wrapping_add(i * 2 * word);
            let base = read_word(emu, entry);
            let len = read_word(emu, entry.wrapping_add(word));
            if len == 0 {
                continue;
            }
            let written = write_from_guest(emu, fd, base, len);
            if written < 0 {
                // An error before anything was written is the result;
                // after, what was written is.
                if total == 0 {
                    total = written;
                }
                break;
            }
            total += written;
            if (written as u64) < len {
                break;
            }
        }
        tracing::info!("sys_writev({}, iovcnt={}) -> {}", fd, iovcnt, total);
        emu.set_syscall_return(total as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysAccess;
impl SimProcedure for SysAccess {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let path = emu.syscall_arg(0);
        let name = read_string(emu, path).unwrap_or_default();
        tracing::info!("sys_access(\"{}\") -> 0 (ok)", name);
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

// ── Memory ───────────────────────────────────────────────────────────────────

/// What a new mapping at `base` holds: zeros, then -- for a file-backed one
/// -- the file's bytes from `offset` on.
///
/// The whole of it, whatever was there before and whatever it may now be
/// used for. This wrote zeros into the first megabyte only, and through the
/// guest's own protection check: a mapping longer than that kept the bytes
/// of whatever had been mapped there earlier (a `malloc` that trusts fresh
/// pages to be zero then returns somebody else's data), and one made without
/// `PROT_WRITE` was not filled at all -- not even with its file.
pub(crate) fn fill_new_mapping(
    emu: &mut Emulator,
    base: u64,
    length: u64,
    file: Option<(u64, usize)>,
) {
    let len = page_align_up(length.max(1));
    // Whatever was translated from these pages is not what they hold now.
    emu.jit_cache.invalidate_range(base, len);
    emu.state.zero_range(base, len);
    let Some((fd, offset)) = file else { return };
    let want = usize::try_from(length).unwrap_or(usize::MAX);
    let data = match emu.vfs.file_bytes(fd, offset, want) {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => return,
    };
    if !data.is_empty() {
        let _ = emu.state.write_ram_unprotected(base, &data);
    }
}

pub struct SysMmap;
impl SimProcedure for SysMmap {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // mmap(addr, length, prot, flags, fd, offset)
        // x86-64 syscall ABI: rdi, rsi, rdx, r10, r8, r9
        let addr = emu.syscall_arg(0);
        let length = emu.syscall_arg(1);
        let prot_bits = emu.syscall_arg(2) as u8;
        let flags = emu.syscall_arg(3);
        let fd = emu.syscall_arg(4) as i64;
        let offset = emu.syscall_arg(5) as usize;
        let page_prot = (prot_bits & 0x07) | prot::VALID;
        let map_fixed = flags & 0x10 != 0; // MAP_FIXED
        let map_anon = flags & 0x20 != 0; // MAP_ANONYMOUS

        if length == 0 {
            // Linux: "EINVAL (since Linux 2.6.12) length was 0." This took it
            // as one byte and mapped a page.
            emu.set_syscall_return((-22i64) as u64)?;
            return Ok(HleResult::Continue);
        }
        let base = emu
            .state
            .page_map
            .mmap_with_hint(addr, length, page_prot, map_fixed);
        if base == 0 {
            // mmap_anon found no free hole; report it rather than handing the
            // guest address 0 as a successful mapping.
            tracing::warn!("sys_mmap(len={length}) -> -ENOMEM");
            emu.set_syscall_return((-12i64) as u64)?; // -ENOMEM
            return Ok(HleResult::Continue);
        }

        fill_new_mapping(
            emu,
            base,
            length,
            (!map_anon && fd >= 0).then_some((fd as u64, offset)),
        );

        emu.set_syscall_return(base)?;
        tracing::info!(
            "sys_mmap(addr=0x{:X}, len={}, prot=0x{:X}, flags=0x{:X}, fd={}, off={}) -> 0x{:X}",
            addr,
            length,
            page_prot,
            flags,
            fd,
            offset,
            base
        );
        Ok(HleResult::Continue)
    }
}

pub struct SysMprotect;
impl SimProcedure for SysMprotect {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let addr = emu.syscall_arg(0);
        let len = emu.syscall_arg(1);
        let prot_bits = emu.syscall_arg(2) as u8;
        // Linux: mprotect on a fully/partially unmapped range returns -ENOMEM.
        // Returning success for unmapped pages made musl mallocng treat a huge
        // corrupted page-count as infinite free pages (silent write failures).
        let ok =
            emu.state
                .page_map
                .mprotect_checked(addr, len.max(1), prot_bits & 0x07 | prot::VALID);
        if ok {
            emu.jit_cache.invalidate_range(addr, len.max(1));
            tracing::info!(
                "sys_mprotect(0x{:X}, {}, 0x{:X}) -> 0",
                addr,
                len,
                prot_bits
            );
            emu.set_syscall_return(0)?;
        } else {
            // -ENOMEM
            tracing::info!(
                "sys_mprotect(0x{:X}, {}, 0x{:X}) -> -ENOMEM",
                addr,
                len,
                prot_bits
            );
            emu.set_syscall_return((-12i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysMunmap;
impl SimProcedure for SysMunmap {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let addr = emu.syscall_arg(0);
        let len = emu.syscall_arg(1);
        emu.state.page_map.unmap_region(addr, len);
        emu.jit_cache.invalidate_range(addr, len.max(1));
        tracing::info!("sys_munmap(0x{:X}, {})", addr, len);
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysBrk;
impl SimProcedure for SysBrk {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let brk = emu.syscall_arg(0);
        let new_brk = emu.state.page_map.set_brk(brk);
        emu.set_syscall_return(new_brk)?;
        tracing::info!("sys_brk(0x{:X}) -> 0x{:X}", brk, new_brk);
        Ok(HleResult::Continue)
    }
}

// ── Process / identity ───────────────────────────────────────────────────────

pub struct SysExit;
impl SimProcedure for SysExit {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // POSIX exit status is 8 bits (see `libc::Exit`'s matching comment;
        // this is the raw-syscall path `exit(2)`/`exit_group(2)` reach for a
        // statically linked binary's own inline `syscall`, and it needs the
        // same mask so both paths agree).
        let code = (emu.syscall_arg(0) as u32) & 0xFF;
        tracing::info!("sys_exit({}). Emulation finished.", code);
        Ok(HleResult::Halt(code))
    }
}

pub struct SysGetpid;
impl SimProcedure for SysGetpid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGettid;
impl SimProcedure for SysGettid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(emu.current_tid)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetuid;
impl SimProcedure for SysGetuid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGeteuid;
impl SimProcedure for SysGeteuid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetgid;
impl SimProcedure for SysGetgid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetegid;
impl SimProcedure for SysGetegid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.set_syscall_return(1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysUname;
impl SimProcedure for SysUname {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // struct utsname — 6 fields of 65 bytes each on Linux
        let buf = emu.syscall_arg(0);
        let field = |s: &str| {
            let mut b = [0u8; 65];
            let bytes = s.as_bytes();
            let n = bytes.len().min(64);
            b[..n].copy_from_slice(&bytes[..n]);
            b
        };
        let mut blob = Vec::with_capacity(65 * 6);
        for s in ["Linux", "fission", "6.0.0-fission", "Fission", "x86_64", ""] {
            blob.extend_from_slice(&field(s));
        }
        ram_write(emu, buf, &blob)?;
        tracing::info!("sys_uname(0x{:X})", buf);
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysArchPrctl;
impl SimProcedure for SysArchPrctl {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // ARCH_SET_GS=0x1001, ARCH_SET_FS=0x1002, ARCH_GET_FS=0x1003, ARCH_GET_GS=0x1004
        let code = emu.syscall_arg(0);
        let addr = emu.syscall_arg(1);
        tracing::info!("sys_arch_prctl(code=0x{:X}, addr=0x{:X})", code, addr);
        match code {
            0x1001 => {
                // ARCH_SET_GS
                emu.set_gs_base(addr);
                emu.set_syscall_return(0)?;
            }
            0x1002 => {
                // ARCH_SET_FS — TLS thread pointer
                emu.set_fs_base(addr);
                emu.set_syscall_return(0)?;
            }
            0x1003 => {
                // ARCH_GET_FS
                if addr != 0 {
                    let _ = ram_write(emu, addr, &emu.fs_base.to_le_bytes());
                }
                emu.set_syscall_return(0)?;
            }
            0x1004 => {
                // ARCH_GET_GS
                if addr != 0 {
                    let _ = ram_write(emu, addr, &emu.gs_base.to_le_bytes());
                }
                emu.set_syscall_return(0)?;
            }
            _ => {
                // Unknown subcode: success no-op (some probes).
                emu.set_syscall_return(0)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

// ── Time / signals (stubs) ───────────────────────────────────────────────────

pub struct SysClockGettime;
impl SimProcedure for SysClockGettime {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tp = emu.syscall_arg(1);
        // timespec { tv_sec: i64, tv_nsec: i64 }
        let sec: i64 = 1_700_000_000;
        let nsec: i64 = 0;
        let mut buf = Vec::with_capacity(16);
        buf.extend_from_slice(&sec.to_le_bytes());
        buf.extend_from_slice(&nsec.to_le_bytes());
        ram_write(emu, tp, &buf)?;
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGettimeofday;
impl SimProcedure for SysGettimeofday {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tv = emu.syscall_arg(0);
        if tv != 0 {
            let sec: i64 = 1_700_000_000;
            let usec: i64 = 0;
            let mut buf = Vec::with_capacity(16);
            buf.extend_from_slice(&sec.to_le_bytes());
            buf.extend_from_slice(&usec.to_le_bytes());
            ram_write(emu, tv, &buf)?;
        }
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysTime;
impl SimProcedure for SysTime {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tloc = emu.syscall_arg(0);
        let t: i64 = 1_700_000_000;
        if tloc != 0 {
            ram_write(emu, tloc, &t.to_le_bytes())?;
        }
        emu.set_syscall_return(t as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigaction;
impl SimProcedure for SysRtSigaction {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // rt_sigaction(signum, act, oldact, sigsetsize)
        use crate::os::linux::signal::SigAction;
        let signum = emu.syscall_arg(0) as i32;
        let act = emu.syscall_arg(1);
        let oldact = emu.syscall_arg(2);
        let sigsetsize = emu.syscall_arg(3);

        if sigsetsize != 8 {
            emu.set_syscall_return((-22i64) as u64)?; // EINVAL
            return Ok(HleResult::Continue);
        }

        if oldact != 0 {
            // Linux kernel_sigaction layout (x86-64, simplified):
            // sa_handler @0 (8), sa_flags @8 (8), sa_restorer @16 (8), sa_mask @24 (8)
            let prev = emu.signals.action(signum);
            let mut buf = [0u8; 32];
            let handler = match prev {
                SigAction::Default => 0u64,
                SigAction::Ignore => 1u64,
                SigAction::Handler(h) => h,
            };
            buf[0..8].copy_from_slice(&handler.to_le_bytes());
            let flags = if signum > 0 && (signum as usize) <= crate::os::linux::signal::NSIG {
                emu.signals.flags[signum as usize]
            } else {
                0
            };
            buf[8..16].copy_from_slice(&flags.to_le_bytes());
            let mask = if signum > 0 && (signum as usize) <= crate::os::linux::signal::NSIG {
                emu.signals.masks[signum as usize]
            } else {
                0
            };
            buf[24..32].copy_from_slice(&mask.to_le_bytes());
            ram_write(emu, oldact, &buf)?;
        }

        if act != 0 {
            let raw = ram_read(emu, act, 32).unwrap_or_else(|_| vec![0; 32]);
            let handler = u64::from_le_bytes(raw[0..8].try_into().unwrap_or([0; 8]));
            let flags = u64::from_le_bytes(raw[8..16].try_into().unwrap_or([0; 8]));
            let mask = if raw.len() >= 32 {
                u64::from_le_bytes(raw[24..32].try_into().unwrap_or([0; 8]))
            } else {
                0
            };
            let action = match handler {
                0 => SigAction::Default,
                1 => SigAction::Ignore,
                h => SigAction::Handler(h),
            };
            if !emu
                .signals
                .set_action_with_mask(signum, action, flags, mask)
            {
                emu.set_syscall_return((-22i64) as u64)?; // EINVAL
                return Ok(HleResult::Continue);
            }
            tracing::info!(
                "sys_rt_sigaction(sig={}, handler=0x{:X}, flags=0x{:X}, mask=0x{:X})",
                signum,
                handler,
                flags,
                mask
            );
        }
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigprocmask;
impl SimProcedure for SysRtSigprocmask {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // rt_sigprocmask(how, set, oldset, sigsetsize)
        let how = emu.syscall_arg(0);
        let set = emu.syscall_arg(1);
        let oldset = emu.syscall_arg(2);
        let sigsetsize = emu.syscall_arg(3);

        if sigsetsize != 8 {
            emu.set_syscall_return((-22i64) as u64)?; // EINVAL
            return Ok(HleResult::Continue);
        }

        if oldset != 0 {
            let mask = emu.signals.blocked_mask();
            ram_write(emu, oldset, &mask.to_le_bytes())?;
        }
        if set != 0 {
            let raw = ram_read(emu, set, 8).unwrap_or_else(|_| vec![0; 8]);
            let new_mask = u64::from_le_bytes(raw.try_into().unwrap_or([0; 8]));
            let cur = emu.signals.blocked_mask();
            let next = match how {
                0 => cur | new_mask,  // SIG_BLOCK
                1 => cur & !new_mask, // SIG_UNBLOCK
                2 => new_mask,        // SIG_SETMASK
                _ => {
                    emu.set_syscall_return((-22i64) as u64)?;
                    return Ok(HleResult::Continue);
                }
            };
            emu.signals.set_blocked_mask(next);
            tracing::debug!("sys_rt_sigprocmask how={} mask=0x{:X}", how, next);
        }
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysKill;
impl SimProcedure for SysKill {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let pid = emu.syscall_arg(0) as i64;
        let sig = emu.syscall_arg(1) as i32;
        tracing::info!("sys_kill(pid={}, sig={})", pid, sig);
        // Only deliver to self (pid 0, -1, or our fake pid 1000).
        if pid == 0 || pid == -1 || pid == 1000 || pid == emu.inst_count as i64 {
            if sig == 0 {
                // Existence check
                emu.set_syscall_return(0)?;
            } else {
                emu.raise_signal(sig);
                emu.set_syscall_return(0)?;
            }
        } else {
            emu.set_syscall_return((-3i64) as u64)?; // ESRCH
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysTkill;
impl SimProcedure for SysTkill {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        tracing::warn!("sys_tkill bypassed the Linux guest-task scheduler");
        emu.set_syscall_return((-38i64) as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigreturn;
impl SimProcedure for SysRtSigreturn {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        if let Some(frame) = emu.signals.pop_frame() {
            let pc = frame.return_pc;
            tracing::info!("sys_rt_sigreturn → PC 0x{:X}", pc);
            if let Some(saved_regs) = frame.saved_registers {
                emu.restore_thread_register_state(&saved_regs)?;
            } else {
                // Best-effort: pop the minimal frame we pushed at delivery.
                let sp_reg = emu.arch.sp_reg;
                if let Ok(sp) = emu.read_register_u64(sp_reg) {
                    let _ = emu
                        .write_register_u64(sp_reg, sp.wrapping_add(emu.arch.pointer_size as u64));
                }
                emu.set_syscall_return(0)?;
            }
            emu.pc = pc;
            emu.pc_override = Some(pc);
        } else {
            tracing::warn!("sys_rt_sigreturn with no saved frame");
            emu.set_syscall_return(0)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysIoctl;
impl SimProcedure for SysIoctl {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.syscall_arg(0) as i32;
        let cmd = emu.syscall_arg(1);
        tracing::debug!("sys_ioctl(fd={}, cmd=0x{:X})", fd, cmd);
        // TCGETS (0x5401) and TIOCGWINSZ (0x5413) query terminal properties.
        // Returning -ENOTTY (-25) ensures isatty() reports false for simulated pipes/files
        // and tools do not divide by zero on uninitialized terminal dimensions.
        const TCGETS: u64 = 0x5401;
        const TIOCGWINSZ: u64 = 0x5413;
        const TCSETS: u64 = 0x5402;
        const TCSETSW: u64 = 0x5403;
        const TCSETSF: u64 = 0x5404;
        const TIOCGPGRP: u64 = 0x540f;
        const TIOCSPGRP: u64 = 0x5410;
        const TIOCSWINSZ: u64 = 0x5414;

        if matches!(
            cmd,
            TCGETS | TIOCGWINSZ | TCSETS | TCSETSW | TCSETSF | TIOCGPGRP | TIOCSPGRP | TIOCSWINSZ
        ) {
            emu.set_syscall_return((-25i64) as u64)?; // -ENOTTY
            return Ok(HleResult::Continue);
        }

        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysFutex;
impl SimProcedure for SysFutex {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let uaddr = emu.syscall_arg(0);
        let op = emu.syscall_arg(1) & 0x7f;
        let val = emu.syscall_arg(2);
        // Support FUTEX_WAIT (0) and FUTEX_WAIT_BITSET (9) value check
        if (op == 0 || op == 9) && uaddr != 0 {
            if let Ok(bytes) = emu.state.read_space(emu.state.ram_space(), uaddr, 4) {
                let current = u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4]));
                if current as u64 != (val & 0xffff_ffff) {
                    emu.set_syscall_return((-11i64) as u64)?; // -EAGAIN
                    return Ok(HleResult::Continue);
                }
            }
        }
        tracing::warn!("sys_futex bypassed the Linux guest-task scheduler");
        emu.set_syscall_return((-38i64) as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetrandom;
impl SimProcedure for SysGetrandom {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let buf = emu.syscall_arg(0);
        let buflen = emu.syscall_arg(1) as usize;
        let mut data = vec![0u8; buflen.min(4096)];
        // Deterministic pseudo-random for reproducibility.
        for (i, b) in data.iter_mut().enumerate() {
            *b = ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as u8;
        }
        ram_write(emu, buf, &data)?;
        emu.set_syscall_return(data.len() as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysSchedYield;
impl SimProcedure for SysSchedYield {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        tracing::warn!("sys_sched_yield bypassed the Linux guest-task scheduler");
        emu.set_syscall_return((-38i64) as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysSetTidAddress;
impl SimProcedure for SysSetTidAddress {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // set_tid_address(tidptr) — store clear_child_tid; return tid.
        let tidptr = emu.syscall_arg(0);
        emu.clear_child_tid = tidptr;
        tracing::info!(
            "sys_set_tid_address(0x{:X}) -> tid={}",
            tidptr,
            emu.current_tid
        );
        emu.set_syscall_return(emu.current_tid)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysPrlimit64;
impl SimProcedure for SysPrlimit64 {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let pid = emu.syscall_arg(0) as i32;
        let resource = emu.syscall_arg(1) as u32;
        let _new_limit = emu.syscall_arg(2);
        let old_limit = emu.syscall_arg(3);

        if pid != 0 && pid != 1000 && pid != emu.current_tid as i32 {
            emu.set_syscall_return((-3i64) as u64)?; // -ESRCH
            return Ok(HleResult::Continue);
        }
        if resource >= 16 {
            emu.set_syscall_return((-22i64) as u64)?; // -EINVAL
            return Ok(HleResult::Continue);
        }

        if old_limit != 0 {
            let (cur, max) = match resource {
                3 => (8 * 1024 * 1024, u64::MAX), // RLIMIT_STACK (8 MiB)
                7 => (1024, 1048576),             // RLIMIT_NOFILE
                _ => (u64::MAX, u64::MAX),
            };
            let mut buf = [0u8; 16];
            buf[..8].copy_from_slice(&cur.to_le_bytes());
            buf[8..].copy_from_slice(&max.to_le_bytes());
            ram_write(emu, old_limit, &buf)?;
        }
        emu.set_syscall_return(0)?;
        Ok(HleResult::Continue)
    }
}

#[cfg(test)]
mod lseek_tests {
    use super::lseek_target;

    #[test]
    fn a_negative_relative_seek_moves_backwards() {
        assert_eq!(lseek_target(10, 100, -4, 1), Some(6));
        assert_eq!(lseek_target(10, 100, -1, 2), Some(99));
        assert_eq!(lseek_target(10, 100, 5, 1), Some(15));
        assert_eq!(lseek_target(10, 100, 0, 0), Some(0));
    }

    #[test]
    fn a_seek_before_the_start_or_with_a_bad_whence_is_an_error() {
        assert_eq!(lseek_target(3, 100, -4, 1), None);
        assert_eq!(lseek_target(0, 100, -1, 0), None);
        assert_eq!(lseek_target(0, 100, 0, 9), None);
    }
}
