//! The host CPU's side: run one instruction for real.
//!
//! A generated stub on an executable page loads the whole input state,
//! executes the instruction, and hits a trapping instruction (`ud2` / `udf`)
//! right after it. The signal handler for that trap reads every register out
//! of the signal context, which is the one place all of them are visible
//! without spending a register on a base pointer, then sends the thread to the
//! stub's exit path with the host's stack pointer and floating-point control
//! restored. A fault anywhere else in the stub (the instruction itself raising
//! SIGILL, SIGSEGV, SIGBUS, SIGFPE or SIGTRAP) takes the same exit with the
//! signal recorded, so one bad instruction costs one case, not the run.
//!
//! The signal frame goes on an alternate stack, because the instruction runs
//! with the guest's stack pointer, which may point anywhere.

#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use crate::model::{BUF_LEN, CpuState};

/// Where the scratch buffer is asked for: low enough for a 39-bit user
/// address space, far from anything else a process maps. Linux honours it
/// (`MAP_FIXED_NOREPLACE`), so reproducers carry the same pointers run to run;
/// macOS treats it as a hint, and [`buf_addr`] is where it landed.
pub const BUF_ADDR: u64 = 0x30_0000_0000;

static BUF_BASE: AtomicU64 = AtomicU64::new(BUF_ADDR);

/// The scratch buffer's address in this process, on both sides.
pub fn buf_addr() -> u64 {
    BUF_BASE.load(Ordering::Relaxed)
}
/// Where the code page is asked for: 1 GiB above the buffer.
pub const CODE_ADDR: u64 = 0x30_4000_0000;
/// Both regions are one 16 KiB page on Apple silicon and four on x86-64, so
/// the bytes past the buffer are unmapped on both sides alike.
pub const REGION_LEN: usize = 16384;
/// Bytes of the code page the stub may use; the rest stays zero.
pub const STUB_MAX: usize = 1024;

pub enum NativeFault {
    Signal { signal: i32, pc: u64 },
}

impl std::fmt::Display for NativeFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NativeFault::Signal { signal, pc } => {
                let name = match *signal {
                    libc::SIGILL => "SIGILL",
                    libc::SIGSEGV => "SIGSEGV",
                    libc::SIGBUS => "SIGBUS",
                    libc::SIGFPE => "SIGFPE",
                    libc::SIGTRAP => "SIGTRAP",
                    libc::SIGALRM => "timeout",
                    _ => "signal",
                };
                write!(f, "{name} at {pc:#x}")
            }
        }
    }
}

/// The raw register file the stub loads from and the handler stores into.
/// One layout serves both ISAs; each uses the fields it has.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct RawState {
    /// x86-64: rax rcx rdx rbx rsp rbp rsi rdi r8-r15 (encoding order).
    /// AArch64: x0-x30, then sp at index 31.
    pub gpr: [u64; 32], // 0
    /// x86-64: RFLAGS. AArch64: NZCV in bits 31..28.
    pub flags: u64, // 256
    /// x86-64: MXCSR. AArch64: FPCR.
    pub control: u64, // 264
    pub pad: [u64; 2],       // 272
    pub vec: [[u8; 16]; 32], // 288
}

pub const OFF_GPR: u32 = 0;
pub const OFF_FLAGS: u32 = 256;
pub const OFF_CONTROL: u32 = 264;
pub const OFF_VEC: u32 = 288;

impl RawState {
    pub const fn zeroed() -> Self {
        Self {
            gpr: [0; 32],
            flags: 0,
            control: 0,
            pad: [0; 2],
            vec: [[0; 16]; 32],
        }
    }
}

// ── State the signal handler needs ──────────────────────────────────────────

static CODE_START: AtomicU64 = AtomicU64::new(0);
static CODE_END: AtomicU64 = AtomicU64::new(0);
static END_PC: AtomicU64 = AtomicU64::new(0);
static EXIT_PC: AtomicU64 = AtomicU64::new(0);
static HOST_SP_SLOT: AtomicU64 = AtomicU64::new(0);
static OUT_PTR: AtomicU64 = AtomicU64::new(0);
static HOST_CONTROL: AtomicU64 = AtomicU64::new(0);
/// 1 after a clean run, minus the signal number after a fault.
static STATUS: AtomicI32 = AtomicI32::new(0);
/// Set while the stub runs: a fault anywhere then (an instruction that
/// escaped the filters and jumped away) is the case's, not the host's.
static IN_STUB: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FAULT_PC: AtomicU64 = AtomicU64::new(0);

const SIGNALS: [i32; 6] = [
    libc::SIGILL,
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGFPE,
    libc::SIGTRAP,
    // The watchdog: an instruction that never finishes (a self-loop the
    // filters missed) is stopped after `WATCHDOG_US`.
    libc::SIGALRM,
];

const WATCHDOG_US: i64 = 500_000;

unsafe fn arm_watchdog(us: i64) {
    unsafe {
        let t = libc::itimerval {
            it_interval: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            it_value: libc::timeval {
                tv_sec: (us / 1_000_000) as _,
                tv_usec: (us % 1_000_000) as _,
            },
        };
        libc::setitimer(libc::ITIMER_REAL, &t, std::ptr::null_mut());
    }
}
static mut OLD_ACTIONS: [Option<libc::sigaction>; 32] = [None; 32];

unsafe extern "C" fn handler(sig: i32, _info: *mut libc::siginfo_t, ctx: *mut c_void) {
    unsafe {
        let pc = arch::ctx_pc(ctx);
        let start = CODE_START.load(Ordering::Relaxed);
        let end = CODE_END.load(Ordering::Relaxed);
        let ours = (start..end).contains(&pc) || IN_STUB.load(Ordering::Relaxed);
        if !ours {
            if sig == libc::SIGALRM {
                return; // the watchdog fired after the stub had finished
            }
            // Not ours: put the previous action back and let the fault recur.
            if let Some(old) = (*std::ptr::addr_of!(OLD_ACTIONS))[sig as usize] {
                libc::sigaction(sig, &old, std::ptr::null_mut());
            }
            return;
        }
        if pc == END_PC.load(Ordering::Relaxed) && sig == libc::SIGILL {
            let out = OUT_PTR.load(Ordering::Relaxed) as *mut RawState;
            arch::save_state(ctx, &mut *out);
            STATUS.store(1, Ordering::Relaxed);
        } else {
            STATUS.store(-sig, Ordering::Relaxed);
            FAULT_PC.store(pc, Ordering::Relaxed);
        }
        IN_STUB.store(false, Ordering::Relaxed);
        let host_sp = *(HOST_SP_SLOT.load(Ordering::Relaxed) as *const u64);
        arch::resume_at_exit(
            ctx,
            EXIT_PC.load(Ordering::Relaxed),
            host_sp,
            HOST_CONTROL.load(Ordering::Relaxed),
        );
    }
}

/// mmap `len` bytes at `addr`, or panic. macOS treats the address as a hint
/// (and places `MAP_JIT` regions itself), so there it may land elsewhere.
unsafe fn map_at(addr: u64, len: usize, exec: bool) -> *mut u8 {
    unsafe {
        #[allow(unused_mut)]
        let mut flags = libc::MAP_PRIVATE | libc::MAP_ANON;
        #[cfg(target_os = "linux")]
        {
            flags |= libc::MAP_FIXED_NOREPLACE;
        }
        #[cfg(target_os = "macos")]
        if exec {
            flags |= libc::MAP_JIT;
        }
        let prot = if exec && cfg!(target_os = "macos") {
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC
        } else {
            libc::PROT_READ | libc::PROT_WRITE
        };
        let p = libc::mmap(addr as *mut c_void, len, prot, flags, -1, 0);
        let exact_required = !cfg!(target_os = "macos");
        assert!(
            p != libc::MAP_FAILED && (p as u64 == addr || !exact_required),
            "could not map {len:#x} bytes at {addr:#x} (got {p:?}); something else is there"
        );
        p as *mut u8
    }
}

pub struct NativeRunner {
    code: *mut u8,
    /// Where the code page landed (see [`map_at`]).
    pub code_addr: u64,
    buf: *mut u8,
    input: Box<RawState>,
    output: Box<RawState>,
    host_sp: Box<u64>,
    altstack: Vec<u8>,
    old_altstack: libc::stack_t,
}

impl NativeRunner {
    pub fn new() -> Self {
        unsafe {
            let code = map_at(CODE_ADDR, REGION_LEN, true);
            let buf = map_at(BUF_ADDR, REGION_LEN, false);
            BUF_BASE.store(buf as u64, Ordering::Relaxed);
            assert_eq!(BUF_LEN, REGION_LEN);
            let mut altstack = vec![0u8; 256 * 1024];
            let new_stack = libc::stack_t {
                ss_sp: altstack.as_mut_ptr() as *mut c_void,
                ss_flags: 0,
                ss_size: altstack.len(),
            };
            let mut old_altstack: libc::stack_t = std::mem::zeroed();
            assert_eq!(libc::sigaltstack(&new_stack, &mut old_altstack), 0);

            for &sig in &SIGNALS {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = handler as *const () as usize;
                action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK | libc::SA_NODEFER;
                libc::sigemptyset(&mut action.sa_mask);
                let mut old: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(sig, &action, &mut old), 0);
                (*std::ptr::addr_of_mut!(OLD_ACTIONS))[sig as usize] = Some(old);
            }
            HOST_CONTROL.store(arch::host_control(), Ordering::Relaxed);
            let code_addr = code as u64;
            CODE_START.store(code_addr, Ordering::Relaxed);
            CODE_END.store(code_addr + STUB_MAX as u64, Ordering::Relaxed);
            Self {
                code,
                code_addr,
                buf,
                input: Box::new(RawState::zeroed()),
                output: Box::new(RawState::zeroed()),
                host_sp: Box::new(0),
                altstack,
                old_altstack,
            }
        }
    }

    /// The stub page for `insn`, and the address `insn` sits at.
    pub fn page_for(&self, insn: &[u8]) -> (Vec<u8>, u64, u64, u64) {
        let stub = arch::build_stub(insn);
        let mut page = stub.bytes;
        assert!(page.len() <= STUB_MAX, "stub too long");
        page.resize(STUB_MAX, 0);
        (
            page,
            self.code_addr + stub.insn_off as u64,
            self.code_addr + stub.end_off as u64,
            self.code_addr + stub.exit_off as u64,
        )
    }

    /// Execute the stub in `page` from `input`.
    pub fn run(
        &mut self,
        page: &[u8],
        end_pc: u64,
        exit_pc: u64,
        input: &CpuState,
    ) -> Result<CpuState, NativeFault> {
        unsafe {
            arch::write_code(self.code, page);
            std::ptr::copy_nonoverlapping(input.buf.as_ptr(), self.buf, BUF_LEN);
            *self.input = arch::to_raw(input);
            *self.output = RawState::zeroed();
            END_PC.store(end_pc, Ordering::Relaxed);
            EXIT_PC.store(exit_pc, Ordering::Relaxed);
            HOST_SP_SLOT.store(&mut *self.host_sp as *mut u64 as u64, Ordering::Relaxed);
            OUT_PTR.store(&mut *self.output as *mut RawState as u64, Ordering::Relaxed);
            STATUS.store(0, Ordering::Relaxed);
            std::sync::atomic::compiler_fence(Ordering::SeqCst);
            let entry: unsafe extern "C" fn(*mut RawState, *mut u64) =
                std::mem::transmute(self.code);
            arm_watchdog(WATCHDOG_US);
            IN_STUB.store(true, Ordering::Relaxed);
            entry(&mut *self.input, &mut *self.host_sp);
            IN_STUB.store(false, Ordering::Relaxed);
            arm_watchdog(0);
            std::sync::atomic::compiler_fence(Ordering::SeqCst);
            match STATUS.load(Ordering::Relaxed) {
                1 => {
                    let mut out = arch::from_raw(&self.output);
                    out.buf = std::slice::from_raw_parts(self.buf, BUF_LEN).to_vec();
                    Ok(out)
                }
                s => Err(NativeFault::Signal {
                    signal: -s,
                    pc: FAULT_PC.load(Ordering::Relaxed),
                }),
            }
        }
    }
}

impl Drop for NativeRunner {
    fn drop(&mut self) {
        unsafe {
            for &sig in &SIGNALS {
                if let Some(old) = (*std::ptr::addr_of!(OLD_ACTIONS))[sig as usize] {
                    libc::sigaction(sig, &old, std::ptr::null_mut());
                }
            }
            libc::sigaltstack(&self.old_altstack, std::ptr::null_mut());
            libc::munmap(self.code as *mut c_void, REGION_LEN);
            libc::munmap(self.buf as *mut c_void, REGION_LEN);
            let _ = &self.altstack;
        }
    }
}

/// The generated stub: the instruction's offset, the trap after it, and the
/// exit path the handler resumes at.
pub struct Stub {
    pub bytes: Vec<u8>,
    pub insn_off: usize,
    pub end_off: usize,
    pub exit_off: usize,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "native_x86_64.rs"]
pub mod arch;

#[cfg(target_arch = "aarch64")]
#[path = "native_aarch64.rs"]
pub mod arch;
