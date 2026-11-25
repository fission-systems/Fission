//! AArch64 (Linux and macOS): stub encoding and signal-context access.

use std::ffi::c_void;

use super::{OFF_CONTROL, OFF_FLAGS, OFF_GPR, OFF_VEC, RawState, Stub};
use crate::model::CpuState;

/// NZCV bit of each entry in `AARCH64.flags` (NG ZR CY OV).
pub const FLAG_BITS: [u32; 4] = [31, 30, 29, 28];

fn stp_pre(rt: u32, rt2: u32, simd: bool) -> u32 {
    let base = if simd { 0x6D80_0000 } else { 0xA980_0000 };
    base | ((-2i32 as u32 & 0x7F) << 15) | (rt2 << 10) | (31 << 5) | rt
}

fn ldp_post(rt: u32, rt2: u32, simd: bool) -> u32 {
    let base = if simd { 0x6CC0_0000 } else { 0xA8C0_0000 };
    base | (2 << 15) | (rt2 << 10) | (31 << 5) | rt
}

fn ldr_x(rt: u32, rn: u32, off: u32) -> u32 {
    0xF940_0000 | ((off / 8) << 10) | (rn << 5) | rt
}

fn ldr_q(rt: u32, rn: u32, off: u32) -> u32 {
    0x3DC0_0000 | ((off / 16) << 10) | (rn << 5) | rt
}

const PAIRS: [(u32, u32); 5] = [(19, 20), (21, 22), (23, 24), (25, 26), (27, 28)];
const DPAIRS: [(u32, u32); 4] = [(8, 9), (10, 11), (12, 13), (14, 15)];

/// ```text
///   stp x29, x30 / x19-x28 / d8-d15 to the host stack
///   mov x2, sp; str x2, [x1]          ; for the handler
///   ldr qN, [x0, vec+16N]             ; N = 0..31
///   ldr x2, [x0, flags]; msr nzcv, x2
///   ldr x2, [x0, control]; msr fpcr, x2; msr fpsr, xzr
///   ldr x2, [x0, sp]; mov sp, x2
///   ldr xN, [x0, 8N]                  ; N = 1..30
///   ldr x0, [x0]
///   <instruction>
///   udf #0                            ; -> handler stores, resumes at exit
/// exit:
///   ldp d14-d8 / x27-x19 / x29, x30; ret
/// ```
pub fn build_stub(insn: &[u8]) -> Stub {
    let mut w: Vec<u32> = Vec::new();
    w.push(stp_pre(29, 30, false));
    for (a, b) in PAIRS {
        w.push(stp_pre(a, b, false));
    }
    for (a, b) in DPAIRS {
        w.push(stp_pre(a, b, true));
    }
    w.push(0x9100_03E2); // mov x2, sp
    w.push(0xF900_0022); // str x2, [x1]
    for n in 0..32 {
        w.push(ldr_q(n, 0, OFF_VEC + 16 * n));
    }
    w.push(ldr_x(2, 0, OFF_FLAGS));
    w.push(0xD51B_4202); // msr nzcv, x2
    w.push(ldr_x(2, 0, OFF_CONTROL));
    w.push(0xD51B_4402); // msr fpcr, x2
    w.push(0xD51B_443F); // msr fpsr, xzr
    w.push(ldr_x(2, 0, OFF_GPR + 8 * 31));
    w.push(0x9100_005F); // mov sp, x2
    for n in 1..=30 {
        w.push(ldr_x(n, 0, OFF_GPR + 8 * n));
    }
    w.push(ldr_x(0, 0, OFF_GPR));
    let mut c: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
    let insn_off = c.len();
    c.extend_from_slice(insn);
    let end_off = c.len();
    c.extend_from_slice(&0u32.to_le_bytes()); // udf #0
    let exit_off = c.len();
    let mut e: Vec<u32> = Vec::new();
    for (a, b) in DPAIRS.iter().rev() {
        e.push(ldp_post(*a, *b, true));
    }
    for (a, b) in PAIRS.iter().rev() {
        e.push(ldp_post(*a, *b, false));
    }
    e.push(ldp_post(29, 30, false));
    e.push(0xD65F_03C0); // ret
    c.extend(e.iter().flat_map(|x| x.to_le_bytes()));
    Stub {
        bytes: c,
        insn_off,
        end_off,
        exit_off,
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_jit_write_protect_np(enabled: i32);
    fn sys_icache_invalidate(start: *mut c_void, len: usize);
}

pub unsafe fn write_code(code: *mut u8, page: &[u8]) {
    unsafe {
        #[cfg(target_os = "macos")]
        {
            pthread_jit_write_protect_np(0);
            std::ptr::copy_nonoverlapping(page.as_ptr(), code, page.len());
            pthread_jit_write_protect_np(1);
            sys_icache_invalidate(code as *mut c_void, page.len());
        }
        #[cfg(target_os = "linux")]
        {
            libc::mprotect(
                code as *mut c_void,
                super::REGION_LEN,
                libc::PROT_READ | libc::PROT_WRITE,
            );
            std::ptr::copy_nonoverlapping(page.as_ptr(), code, page.len());
            libc::mprotect(
                code as *mut c_void,
                super::REGION_LEN,
                libc::PROT_READ | libc::PROT_EXEC,
            );
            // Data and instruction caches are not coherent on AArch64.
            let mut line = code as usize;
            let end = code as usize + page.len();
            while line < end {
                std::arch::asm!("dc cvau, {0}", in(reg) line);
                line += 16;
            }
            std::arch::asm!("dsb ish");
            let mut line = code as usize;
            while line < end {
                std::arch::asm!("ic ivau, {0}", in(reg) line);
                line += 16;
            }
            std::arch::asm!("dsb ish", "isb");
        }
    }
}

pub fn host_control() -> u64 {
    let fpcr: u64;
    unsafe { std::arch::asm!("mrs {0}, fpcr", out(reg) fpcr) };
    fpcr
}

pub fn to_raw(s: &CpuState) -> RawState {
    let mut r = RawState::zeroed();
    r.gpr = s.gpr;
    let mut nzcv = 0u64;
    for (i, bit) in FLAG_BITS.iter().enumerate() {
        if s.flags[i] != 0 {
            nzcv |= 1 << bit;
        }
    }
    r.flags = nzcv;
    r.control = 0; // FPCR: round to nearest, no flush-to-zero, no default NaN
    r.vec = s.vec;
    r
}

pub fn from_raw(r: &RawState) -> CpuState {
    let mut s = CpuState::zeroed();
    s.gpr = r.gpr;
    for (i, bit) in FLAG_BITS.iter().enumerate() {
        s.flags[i] = ((r.flags >> bit) & 1) as u8;
    }
    s.vec = r.vec;
    s
}

// ── Signal context ──────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod ctx {
    use super::*;

    const FPSIMD_MAGIC: u32 = 0x4650_8001;

    unsafe fn mc(ctx: *mut c_void) -> *mut libc::mcontext_t {
        unsafe { &mut (*(ctx as *mut libc::ucontext_t)).uc_mcontext }
    }

    /// The `fpsimd_context` record in `__reserved`: {magic, size, fpsr, fpcr, vregs[32]}.
    unsafe fn fpsimd(ctx: *mut c_void) -> Option<*mut u8> {
        unsafe {
            // `__reserved` follows `pstate` and is 16-byte aligned: offset 288.
            let mut p = (mc(ctx) as *mut u8).add(288);
            let end = p.add(4096);
            while p < end {
                let magic = (p as *const u32).read();
                let size = (p.add(4) as *const u32).read() as usize;
                if magic == FPSIMD_MAGIC {
                    return Some(p);
                }
                if magic == 0 || size == 0 {
                    return None;
                }
                p = p.add(size);
            }
            None
        }
    }

    pub unsafe fn ctx_pc(ctx: *mut c_void) -> u64 {
        unsafe { (*mc(ctx)).pc }
    }

    pub unsafe fn save_state(ctx: *mut c_void, out: &mut RawState) {
        unsafe {
            let m = &*mc(ctx);
            out.gpr[..31].copy_from_slice(&m.regs);
            out.gpr[31] = m.sp;
            out.flags = m.pstate & 0xF000_0000;
            if let Some(f) = fpsimd(ctx) {
                out.control = u64::from((f.add(12) as *const u32).read());
                for n in 0..32 {
                    std::ptr::copy_nonoverlapping(f.add(16 + 16 * n), out.vec[n].as_mut_ptr(), 16);
                }
            }
        }
    }

    pub unsafe fn resume_at_exit(ctx: *mut c_void, exit_pc: u64, host_sp: u64, control: u64) {
        unsafe {
            let m = &mut *mc(ctx);
            m.pc = exit_pc;
            m.sp = host_sp;
            if let Some(f) = fpsimd(ctx) {
                (f.add(8) as *mut u32).write(0);
                (f.add(12) as *mut u32).write(control as u32);
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod ctx {
    use super::*;

    #[repr(C)]
    struct ThreadState {
        x: [u64; 29],
        fp: u64,
        lr: u64,
        sp: u64,
        pc: u64,
        cpsr: u32,
        pad: u32,
    }

    #[repr(C, align(16))]
    struct NeonState {
        v: [u128; 32],
        fpsr: u32,
        fpcr: u32,
    }

    #[repr(C)]
    struct MContext {
        far: u64,
        esr: u32,
        exception: u32,
        ss: ThreadState,
        ns: NeonState,
    }

    #[repr(C)]
    struct UContext {
        onstack: i32,
        sigmask: u32,
        ss_sp: *mut c_void,
        ss_size: usize,
        ss_flags: i32,
        link: *mut c_void,
        mcsize: usize,
        mcontext: *mut MContext,
    }

    unsafe fn mc(ctx: *mut c_void) -> *mut MContext {
        unsafe { (*(ctx as *mut UContext)).mcontext }
    }

    pub unsafe fn ctx_pc(ctx: *mut c_void) -> u64 {
        unsafe { (*mc(ctx)).ss.pc }
    }

    pub unsafe fn save_state(ctx: *mut c_void, out: &mut RawState) {
        unsafe {
            let m = &*mc(ctx);
            out.gpr[..29].copy_from_slice(&m.ss.x);
            out.gpr[29] = m.ss.fp;
            out.gpr[30] = m.ss.lr;
            out.gpr[31] = m.ss.sp;
            out.flags = u64::from(m.ss.cpsr) & 0xF000_0000;
            out.control = u64::from(m.ns.fpcr);
            for n in 0..32 {
                out.vec[n] = m.ns.v[n].to_le_bytes();
            }
        }
    }

    pub unsafe fn resume_at_exit(ctx: *mut c_void, exit_pc: u64, host_sp: u64, control: u64) {
        unsafe {
            let m = &mut *mc(ctx);
            m.ss.pc = exit_pc;
            m.ss.sp = host_sp;
            m.ns.fpsr = 0;
            m.ns.fpcr = control as u32;
        }
    }
}

pub use ctx::{ctx_pc, resume_at_exit, save_state};
