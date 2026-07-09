//! Linux x86-64 syscall HLE (user-mode).
//!
//! Coverage is intentionally pragmatic: enough for static musl/glibc startup
//! stubs and simple utilities. Expand by adding `SimProcedure`s and registering
//! numbers in `LinuxEnv::new`. QEMU linux-user is a structural reference only.

use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::linux::abi::TargetStat;
use crate::os::linux::libc::read_string;
use crate::os::procedure::SimProcedure;
use crate::pcode::page_map::{page_align_down, page_align_up, prot};
use anyhow::Result;

fn ram_write(emu: &mut Emulator, addr: u64, data: &[u8]) -> Result<()> {
    let ram = emu.state.ram_space();
    emu.state.write_space(ram, addr, data)
}

fn ram_read(emu: &mut Emulator, addr: u64, size: usize) -> Result<Vec<u8>> {
    let ram = emu.state.ram_space();
    emu.state.read_space(ram, addr, size)
}

// ── File I/O ─────────────────────────────────────────────────────────────────

pub struct SysRead;
impl SimProcedure for SysRead {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let buf = emu.read_register_u64("RSI").unwrap_or(0);
        let count = emu.read_register_u64("RDX").unwrap_or(0);

        tracing::info!("sys_read({}, 0x{:X}, {})", fd, buf, count);

        match emu.vfs.read(fd, count as usize) {
            Ok(data) => {
                let bytes_read = data.len();
                if bytes_read > 0 {
                    ram_write(emu, buf, &data)?;
                }
                if fd == 0 {
                    for i in 0..bytes_read {
                        let node = emu
                            .solver
                            .register_var(format!("stdin_{}", buf + (i as u64)), 1);
                        emu.state
                            .set_shadow_memory(emu.state.ram_space(), buf + (i as u64), node);
                    }
                }
                emu.write_register_u64("RAX", bytes_read as u64)?;
            }
            Err(_) => {
                emu.write_register_u64("RAX", (-1i64) as u64)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysWrite;
impl SimProcedure for SysWrite {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let buf = emu.read_register_u64("RSI").unwrap_or(0);
        let count = emu.read_register_u64("RDX").unwrap_or(0);

        tracing::info!("sys_write({}, 0x{:X}, {})", fd, buf, count);

        let data = ram_read(emu, buf, count as usize).unwrap_or_default();
        match emu.vfs.write(fd, &data) {
            Ok(written) => {
                if fd == 1 || fd == 2 {
                    print!("{}", String::from_utf8_lossy(&data[..written]));
                }
                emu.write_register_u64("RAX", written as u64)?;
            }
            Err(_) => {
                emu.write_register_u64("RAX", (-1i64) as u64)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysOpen;
impl SimProcedure for SysOpen {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let filename_ptr = emu.read_register_u64("RDI").unwrap_or(0);
        let filename = read_string(emu, filename_ptr).unwrap_or_else(|_| "unknown".into());
        tracing::info!("sys_open(\"{}\")", filename);
        let fd = emu.vfs.open(&filename, Vec::new());
        emu.write_register_u64("RAX", fd)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysOpenat;
impl SimProcedure for SysOpenat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // openat(dirfd, pathname, flags, mode) — x86-64: rdi, rsi, rdx, r10
        let _dirfd = emu.read_register_u64("RDI").unwrap_or(0) as i64;
        let pathname = emu.read_register_u64("RSI").unwrap_or(0);
        let flags = emu.read_register_u64("RDX").unwrap_or(0);
        let filename = read_string(emu, pathname).unwrap_or_else(|_| "unknown".into());
        tracing::info!("sys_openat(dirfd={}, \"{}\", flags=0x{:X})", _dirfd, filename, flags);
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
                emu.write_register_u64("RAX", (-2i64) as u64)?;
                return Ok(HleResult::Continue);
            }
        }
        emu.write_register_u64("RAX", fd)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysClose;
impl SimProcedure for SysClose {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        tracing::info!("sys_close({})", fd);
        if emu.vfs.close(fd).is_ok() {
            emu.write_register_u64("RAX", 0)?;
        } else {
            emu.write_register_u64("RAX", (-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysFstat;
impl SimProcedure for SysFstat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let statbuf = emu.read_register_u64("RSI").unwrap_or(0);
        tracing::info!("sys_fstat({}, 0x{:X})", fd, statbuf);

        if let Some(file) = emu.vfs.files.get(&fd) {
            let mut target_st = TargetStat::default();
            target_st.st_size = file.content.len() as i64;
            target_st.st_mode = 0x81B4;
            ram_write(emu, statbuf, &target_st.to_bytes())?;
            emu.write_register_u64("RAX", 0)?;
        } else if fd <= 2 {
            let mut target_st = TargetStat::default();
            target_st.st_mode = 0x2180; // char device-ish for stdio
            ram_write(emu, statbuf, &target_st.to_bytes())?;
            emu.write_register_u64("RAX", 0)?;
        } else {
            emu.write_register_u64("RAX", (-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysNewfstatat;
impl SimProcedure for SysNewfstatat {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // newfstatat(dirfd, pathname, statbuf, flags)
        let pathname = emu.read_register_u64("RSI").unwrap_or(0);
        let statbuf = emu.read_register_u64("RDX").unwrap_or(0);
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
        ram_write(emu, statbuf, &target_st.to_bytes())?;
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysLseek;
impl SimProcedure for SysLseek {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let offset = emu.read_register_u64("RSI").unwrap_or(0) as usize;
        let whence = emu.read_register_u64("RDX").unwrap_or(0);
        tracing::info!("sys_lseek({}, {}, {})", fd, offset, whence);
        if let Some(file) = emu.vfs.files.get_mut(&fd) {
            let new = match whence {
                0 => offset, // SEEK_SET
                1 => file.cursor.saturating_add(offset),
                2 => file.content.len().saturating_add(offset),
                _ => {
                    emu.write_register_u64("RAX", (-1i64) as u64)?;
                    return Ok(HleResult::Continue);
                }
            };
            file.seek(new);
            emu.write_register_u64("RAX", new as u64)?;
        } else {
            emu.write_register_u64("RAX", (-1i64) as u64)?;
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysWritev;
impl SimProcedure for SysWritev {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // writev(fd, iov, iovcnt) — iovec is {void* base; size_t len}
        let fd = emu.read_register_u64("RDI").unwrap_or(0);
        let iov = emu.read_register_u64("RSI").unwrap_or(0);
        let iovcnt = emu.read_register_u64("RDX").unwrap_or(0) as usize;
        let mut total = 0usize;
        for i in 0..iovcnt.min(64) {
            let base_off = iov + (i as u64) * 16;
            let base = u64::from_le_bytes(
                ram_read(emu, base_off, 8)
                    .unwrap_or_else(|_| vec![0; 8])
                    .try_into()
                    .unwrap_or([0; 8]),
            );
            let len = u64::from_le_bytes(
                ram_read(emu, base_off + 8, 8)
                    .unwrap_or_else(|_| vec![0; 8])
                    .try_into()
                    .unwrap_or([0; 8]),
            ) as usize;
            if len == 0 {
                continue;
            }
            let data = ram_read(emu, base, len).unwrap_or_default();
            if let Ok(n) = emu.vfs.write(fd, &data) {
                if fd == 1 || fd == 2 {
                    print!("{}", String::from_utf8_lossy(&data[..n]));
                }
                total += n;
            }
        }
        tracing::info!("sys_writev({}, iovcnt={}) -> {}", fd, iovcnt, total);
        emu.write_register_u64("RAX", total as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysAccess;
impl SimProcedure for SysAccess {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let path = emu.read_register_u64("RDI").unwrap_or(0);
        let name = read_string(emu, path).unwrap_or_default();
        tracing::info!("sys_access(\"{}\") -> 0 (ok)", name);
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

// ── Memory ───────────────────────────────────────────────────────────────────

pub struct SysMmap;
impl SimProcedure for SysMmap {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // mmap(addr, length, prot, flags, fd, offset)
        // x86-64 syscall ABI: rdi, rsi, rdx, r10, r8, r9
        let addr = emu.read_register_u64("RDI").unwrap_or(0);
        let length = emu.read_register_u64("RSI").unwrap_or(0);
        let prot_bits = emu.read_register_u64("RDX").unwrap_or(0) as u8;
        let flags = emu.read_register_u64("R10").unwrap_or(0);
        let fd = emu.read_register_u64("R8").unwrap_or(u64::MAX) as i64;
        let offset = emu.read_register_u64("R9").unwrap_or(0) as usize;
        let page_prot = (prot_bits & 0x07) | prot::VALID;
        let map_fixed = flags & 0x10 != 0; // MAP_FIXED
        let map_anon = flags & 0x20 != 0; // MAP_ANONYMOUS

        let base = if addr == 0 && !map_fixed {
            emu.state.page_map.mmap_anon(length.max(1), page_prot)
        } else {
            emu.state
                .page_map
                .map_region(addr, length.max(1), page_prot, true);
            page_align_down(addr)
        };

        let len = page_align_up(length.max(1));
        // File-backed mmap: copy from VFS fd when not MAP_ANONYMOUS.
        if !map_anon && fd >= 0 {
            let want = length.max(1) as usize;
            match emu.vfs.peek(fd as u64, offset, want) {
                Ok(data) => {
                    let _ = ram_write(emu, base, &data);
                }
                Err(_) => {
                    let fill = len.min(0x10_0000) as usize;
                    if fill > 0 {
                        let zeros = vec![0u8; fill];
                        let _ = ram_write(emu, base, &zeros);
                    }
                }
            }
        } else {
            let fill = len.min(0x10_0000) as usize;
            if fill > 0 {
                let zeros = vec![0u8; fill];
                let _ = ram_write(emu, base, &zeros);
            }
        }

        emu.write_register_u64("RAX", base)?;
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
        let addr = emu.read_register_u64("RDI").unwrap_or(0);
        let len = emu.read_register_u64("RSI").unwrap_or(0);
        let prot_bits = emu.read_register_u64("RDX").unwrap_or(0) as u8;
        emu.state
            .page_map
            .mprotect(addr, len, prot_bits & 0x07 | prot::VALID);
        // SMC: any drop of EXEC or change may need invalidation — invalidate range pages.
        let mut page = page_align_down(addr);
        let end = page_align_up(addr.saturating_add(len.max(1)));
        while page < end {
            emu.jit_cache.invalidate_page(page);
            page = page.saturating_add(0x1000);
        }
        tracing::info!("sys_mprotect(0x{:X}, {}, 0x{:X})", addr, len, prot_bits);
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysMunmap;
impl SimProcedure for SysMunmap {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let addr = emu.read_register_u64("RDI").unwrap_or(0);
        let len = emu.read_register_u64("RSI").unwrap_or(0);
        emu.state.page_map.unmap_region(addr, len);
        let mut page = page_align_down(addr);
        let end = page_align_up(addr.saturating_add(len.max(1)));
        while page < end {
            emu.jit_cache.invalidate_page(page);
            page = page.saturating_add(0x1000);
        }
        tracing::info!("sys_munmap(0x{:X}, {})", addr, len);
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysBrk;
impl SimProcedure for SysBrk {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let brk = emu.read_register_u64("RDI").unwrap_or(0);
        let new_brk = emu.state.page_map.set_brk(brk);
        emu.write_register_u64("RAX", new_brk)?;
        tracing::info!("sys_brk(0x{:X}) -> 0x{:X}", brk, new_brk);
        Ok(HleResult::Continue)
    }
}

// ── Process / identity ───────────────────────────────────────────────────────

pub struct SysExit;
impl SimProcedure for SysExit {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let code = emu.read_register_u64("RDI").unwrap_or(0) as u32;
        tracing::info!("sys_exit({}). Emulation finished.", code);
        Ok(HleResult::Halt(code))
    }
}

pub struct SysGetpid;
impl SimProcedure for SysGetpid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGettid;
impl SimProcedure for SysGettid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetuid;
impl SimProcedure for SysGetuid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGeteuid;
impl SimProcedure for SysGeteuid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetgid;
impl SimProcedure for SysGetgid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetegid;
impl SimProcedure for SysGetegid {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysUname;
impl SimProcedure for SysUname {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // struct utsname — 6 fields of 65 bytes each on Linux
        let buf = emu.read_register_u64("RDI").unwrap_or(0);
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
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysArchPrctl;
impl SimProcedure for SysArchPrctl {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // ARCH_SET_FS = 0x1002, ARCH_GET_FS = 0x1003, ARCH_SET_GS = 0x1001
        let code = emu.read_register_u64("RDI").unwrap_or(0);
        let addr = emu.read_register_u64("RSI").unwrap_or(0);
        tracing::info!("sys_arch_prctl(code=0x{:X}, addr=0x{:X})", code, addr);
        // Store FS base in a fixed guest location for segment_fs userops later.
        match code {
            0x1002 => {
                // SET_FS — remember in tick_count high bit area / vfs side channel
                emu.tick_count = addr; // temporary store; better field later
                emu.write_register_u64("RAX", 0)?;
            }
            0x1003 => {
                // GET_FS
                let _ = ram_write(emu, addr, &emu.tick_count.to_le_bytes());
                emu.write_register_u64("RAX", 0)?;
            }
            _ => {
                emu.write_register_u64("RAX", 0)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

// ── Time / signals (stubs) ───────────────────────────────────────────────────

pub struct SysClockGettime;
impl SimProcedure for SysClockGettime {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tp = emu.read_register_u64("RSI").unwrap_or(0);
        // timespec { tv_sec: i64, tv_nsec: i64 }
        let sec: i64 = 1_700_000_000;
        let nsec: i64 = 0;
        let mut buf = Vec::with_capacity(16);
        buf.extend_from_slice(&sec.to_le_bytes());
        buf.extend_from_slice(&nsec.to_le_bytes());
        ram_write(emu, tp, &buf)?;
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGettimeofday;
impl SimProcedure for SysGettimeofday {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tv = emu.read_register_u64("RDI").unwrap_or(0);
        if tv != 0 {
            let sec: i64 = 1_700_000_000;
            let usec: i64 = 0;
            let mut buf = Vec::with_capacity(16);
            buf.extend_from_slice(&sec.to_le_bytes());
            buf.extend_from_slice(&usec.to_le_bytes());
            ram_write(emu, tv, &buf)?;
        }
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysTime;
impl SimProcedure for SysTime {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let tloc = emu.read_register_u64("RDI").unwrap_or(0);
        let t: i64 = 1_700_000_000;
        if tloc != 0 {
            ram_write(emu, tloc, &t.to_le_bytes())?;
        }
        emu.write_register_u64("RAX", t as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigaction;
impl SimProcedure for SysRtSigaction {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // rt_sigaction(signum, act, oldact, sigsetsize)
        use crate::os::linux::signal::SigAction;
        let signum = emu.read_register_u64("RDI").unwrap_or(0) as i32;
        let act = emu.read_register_u64("RSI").unwrap_or(0);
        let oldact = emu.read_register_u64("RDX").unwrap_or(0);

        if oldact != 0 {
            // Linux kernel_sigaction layout (x86-64, simplified):
            // sa_handler @0 (8), sa_flags @8 (8), sa_restorer @16 (8), sa_mask @24
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
            ram_write(emu, oldact, &buf)?;
        }

        if act != 0 {
            let raw = ram_read(emu, act, 16).unwrap_or_else(|_| vec![0; 16]);
            let handler = u64::from_le_bytes(raw[0..8].try_into().unwrap_or([0; 8]));
            let flags = u64::from_le_bytes(raw[8..16].try_into().unwrap_or([0; 8]));
            let action = match handler {
                0 => SigAction::Default,
                1 => SigAction::Ignore,
                h => SigAction::Handler(h),
            };
            if !emu.signals.set_action(signum, action, flags) {
                emu.write_register_u64("RAX", (-22i64) as u64)?; // EINVAL
                return Ok(HleResult::Continue);
            }
            tracing::info!(
                "sys_rt_sigaction(sig={}, handler=0x{:X}, flags=0x{:X})",
                signum,
                handler,
                flags
            );
        }
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigprocmask;
impl SimProcedure for SysRtSigprocmask {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // rt_sigprocmask(how, set, oldset, sigsetsize)
        let how = emu.read_register_u64("RDI").unwrap_or(0);
        let set = emu.read_register_u64("RSI").unwrap_or(0);
        let oldset = emu.read_register_u64("RDX").unwrap_or(0);

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
                    emu.write_register_u64("RAX", (-22i64) as u64)?;
                    return Ok(HleResult::Continue);
                }
            };
            emu.signals.set_blocked_mask(next);
            tracing::debug!("sys_rt_sigprocmask how={} mask=0x{:X}", how, next);
        }
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysKill;
impl SimProcedure for SysKill {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let pid = emu.read_register_u64("RDI").unwrap_or(0) as i64;
        let sig = emu.read_register_u64("RSI").unwrap_or(0) as i32;
        tracing::info!("sys_kill(pid={}, sig={})", pid, sig);
        // Only deliver to self (pid 0, -1, or our fake pid 1000).
        if pid == 0 || pid == -1 || pid == 1000 || pid == emu.inst_count as i64 {
            if sig == 0 {
                // Existence check
                emu.write_register_u64("RAX", 0)?;
            } else {
                emu.raise_signal(sig);
                emu.write_register_u64("RAX", 0)?;
            }
        } else {
            emu.write_register_u64("RAX", (-3i64) as u64)?; // ESRCH
        }
        Ok(HleResult::Continue)
    }
}

pub struct SysTkill;
impl SimProcedure for SysTkill {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let _tid = emu.read_register_u64("RDI").unwrap_or(0);
        let sig = emu.read_register_u64("RSI").unwrap_or(0) as i32;
        tracing::info!("sys_tkill(sig={})", sig);
        if sig != 0 {
            emu.raise_signal(sig);
        }
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysRtSigreturn;
impl SimProcedure for SysRtSigreturn {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        if let Some(pc) = emu.signals.sigreturn() {
            tracing::info!("sys_rt_sigreturn → PC 0x{:X}", pc);
            // Best-effort: pop the minimal frame we pushed at delivery.
            let sp_reg = emu.arch.sp_reg;
            if let Ok(sp) = emu.read_register_u64(sp_reg) {
                let _ = emu.write_register_u64(
                    sp_reg,
                    sp.wrapping_add(emu.arch.pointer_size as u64),
                );
            }
            emu.pc = pc;
            emu.pc_override = Some(pc);
        } else {
            tracing::warn!("sys_rt_sigreturn with no saved frame");
        }
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysIoctl;
impl SimProcedure for SysIoctl {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        tracing::debug!("sys_ioctl (stub -> 0)");
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysFutex;
impl SimProcedure for SysFutex {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // WAKE/WAIT stubs: return 0 so single-threaded guests proceed.
        tracing::debug!("sys_futex (stub -> 0)");
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysGetrandom;
impl SimProcedure for SysGetrandom {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        let buf = emu.read_register_u64("RDI").unwrap_or(0);
        let buflen = emu.read_register_u64("RSI").unwrap_or(0) as usize;
        let mut data = vec![0u8; buflen.min(4096)];
        // Deterministic pseudo-random for reproducibility.
        for (i, b) in data.iter_mut().enumerate() {
            *b = ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as u8;
        }
        ram_write(emu, buf, &data)?;
        emu.write_register_u64("RAX", data.len() as u64)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysSchedYield;
impl SimProcedure for SysSchedYield {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysSetTidAddress;
impl SimProcedure for SysSetTidAddress {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        // Returns caller's tid.
        emu.write_register_u64("RAX", 1000)?;
        Ok(HleResult::Continue)
    }
}

pub struct SysPrlimit64;
impl SimProcedure for SysPrlimit64 {
    fn run(&self, emu: &mut Emulator) -> Result<HleResult> {
        emu.write_register_u64("RAX", 0)?;
        Ok(HleResult::Continue)
    }
}
