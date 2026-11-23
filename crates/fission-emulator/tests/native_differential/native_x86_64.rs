//! x86-64 Linux: stub encoding and signal-context access.

use std::ffi::c_void;

use super::{OFF_CONTROL, OFF_FLAGS, OFF_GPR, OFF_VEC, RawState, Stub};
use crate::model::CpuState;

/// RFLAGS bit of each entry in `X86_64.flags` (CF PF AF ZF SF OF DF).
pub const FLAG_BITS: [u32; 7] = [0, 2, 4, 6, 7, 11, 10];
const DF: u64 = 1 << 10;
const TF: u64 = 1 << 8;
const AC: u64 = 1 << 18;

fn d32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// ```text
///   push rbx; push rbp; push r12..r15     ; callee-saved, restored by the exit
///   mov [rsi], rsp                        ; for the handler
///   mov rax, rdi                          ; rax = &RawState
///   ldmxcsr [rax+control]
///   movdqu xmmN, [rax+vec+16N]            ; N = 0..15
///   push qword [rax+flags]; popfq
///   mov reg, [rax+8*reg]                  ; every GPR but rax, rsp included
///   mov rax, [rax]
///   <instruction>
///   ud2                                   ; -> handler stores, resumes at exit
/// exit:
///   pop r15..r12; pop rbp; pop rbx; ret
/// ```
pub fn build_stub(insn: &[u8]) -> Stub {
    let mut c = vec![0x53, 0x55, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57];
    c.extend_from_slice(&[0x48, 0x89, 0x26]); // mov [rsi], rsp
    c.extend_from_slice(&[0x48, 0x89, 0xF8]); // mov rax, rdi
    c.extend_from_slice(&[0x0F, 0xAE, 0x90]); // ldmxcsr [rax+d32]
    d32(&mut c, OFF_CONTROL);
    for n in 0..16u32 {
        c.push(0xF3);
        if n >= 8 {
            c.push(0x44);
        }
        c.extend_from_slice(&[0x0F, 0x6F, 0x80 | (((n & 7) as u8) << 3)]);
        d32(&mut c, OFF_VEC + 16 * n);
    }
    c.extend_from_slice(&[0xFF, 0xB0]); // push qword [rax+d32]
    d32(&mut c, OFF_FLAGS);
    c.push(0x9D); // popfq
    for r in 1..16u32 {
        c.push(0x48 | if r >= 8 { 0x04 } else { 0 });
        c.extend_from_slice(&[0x8B, 0x80 | (((r & 7) as u8) << 3)]);
        d32(&mut c, OFF_GPR + 8 * r);
    }
    c.extend_from_slice(&[0x48, 0x8B, 0x80]); // mov rax, [rax+d32]
    d32(&mut c, OFF_GPR);
    let insn_off = c.len();
    c.extend_from_slice(insn);
    let end_off = c.len();
    c.extend_from_slice(&[0x0F, 0x0B]); // ud2
    let exit_off = c.len();
    c.extend_from_slice(&[
        0x41, 0x5F, 0x41, 0x5E, 0x41, 0x5D, 0x41, 0x5C, 0x5D, 0x5B, 0xC3,
    ]);
    Stub {
        bytes: c,
        insn_off,
        end_off,
        exit_off,
    }
}

/// Linux's x86-64 code pages are plain mappings: flip them writable, write,
/// flip them back, so the guest cannot write the stub (Fission maps it RX).
pub unsafe fn write_code(code: *mut u8, page: &[u8]) {
    unsafe {
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
    }
}

pub fn host_control() -> u64 {
    0x1F80
}

pub fn to_raw(s: &CpuState) -> RawState {
    let mut r = RawState::zeroed();
    r.gpr[..16].copy_from_slice(&s.gpr[..16]);
    // Reserved bit 1 and IF are always set in user mode.
    let mut flags = 0x202u64;
    for (i, bit) in FLAG_BITS.iter().enumerate() {
        if s.flags[i] != 0 {
            flags |= 1 << bit;
        }
    }
    r.flags = flags;
    r.control = 0x1F80;
    r.vec[..16].copy_from_slice(&s.vec[..16]);
    r
}

pub fn from_raw(r: &RawState) -> CpuState {
    let mut s = CpuState::zeroed();
    s.gpr[..16].copy_from_slice(&r.gpr[..16]);
    for (i, bit) in FLAG_BITS.iter().enumerate() {
        s.flags[i] = ((r.flags >> bit) & 1) as u8;
    }
    s.vec[..16].copy_from_slice(&r.vec[..16]);
    s
}

const GREG_OF: [i32; 16] = [
    libc::REG_RAX,
    libc::REG_RCX,
    libc::REG_RDX,
    libc::REG_RBX,
    libc::REG_RSP,
    libc::REG_RBP,
    libc::REG_RSI,
    libc::REG_RDI,
    libc::REG_R8,
    libc::REG_R9,
    libc::REG_R10,
    libc::REG_R11,
    libc::REG_R12,
    libc::REG_R13,
    libc::REG_R14,
    libc::REG_R15,
];

pub unsafe fn ctx_pc(ctx: *mut c_void) -> u64 {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as u64
    }
}

pub unsafe fn save_state(ctx: *mut c_void, out: &mut RawState) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let g = &(*uc).uc_mcontext.gregs;
        for (i, &reg) in GREG_OF.iter().enumerate() {
            out.gpr[i] = g[reg as usize] as u64;
        }
        out.flags = g[libc::REG_EFL as usize] as u64;
        let fp = (*uc).uc_mcontext.fpregs;
        if !fp.is_null() {
            out.control = u64::from((*fp).mxcsr);
            for i in 0..16 {
                let e = (*fp)._xmm[i].element;
                for (j, word) in e.iter().enumerate() {
                    out.vec[i][4 * j..4 * j + 4].copy_from_slice(&word.to_le_bytes());
                }
            }
        }
    }
}

pub unsafe fn resume_at_exit(ctx: *mut c_void, exit_pc: u64, host_sp: u64, control: u64) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let g = &mut (*uc).uc_mcontext.gregs;
        g[libc::REG_RIP as usize] = exit_pc as i64;
        g[libc::REG_RSP as usize] = host_sp as i64;
        g[libc::REG_EFL as usize] &= !((DF | TF | AC) as i64);
        let fp = (*uc).uc_mcontext.fpregs;
        if !fp.is_null() {
            (*fp).mxcsr = control as u32;
        }
    }
}
