//! Which registers a Linux syscall uses, and what its numbers mean.
//!
//! Both differ per architecture, and both were hard-coded to x86-64. Every one
//! of the thirty-eight handlers read `RDI`/`RSI`/`RDX` by name and wrote `RAX`
//! by name, and the registry was keyed on x86-64's numbering -- so an aarch64
//! image could not make a single syscall, whatever else worked.
//!
//! # Numbers
//!
//! x86-64's table is historical. Everything newer -- aarch64, riscv, and the
//! 32-bit architectures' `*_time64` variants -- uses the *generic* table in
//! `include/uapi/asm-generic/unistd.h`, where `write` is 64 and not 1.
//!
//! Rather than registering every handler twice, an architecture that uses the
//! generic table translates into the x86-64 numbering the registry already
//! speaks. The direction matters: the registry's keys are an internal
//! identifier, and x86-64's numbers are as good an identifier as any -- what
//! must not happen is two tables of handlers that drift apart.
//!
//! Where the two tables disagree about what exists, the translation says so
//! rather than guessing. aarch64 has no `open`, only `openat`; no `access`,
//! only `faccessat`; no `arch_prctl`, because it puts the thread pointer in a
//! register. A number with no counterpart is left alone and reported unknown,
//! which is the truthful answer.

use crate::arch::ArchInfo;

/// The registers a syscall is made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallAbi {
    /// Where the syscall number is.
    pub number: &'static str,
    /// Where its arguments are, in order.
    pub args: [&'static str; 6],
    /// Where its result goes.
    pub result: &'static str,
    /// Whether this architecture uses the generic (asm-generic) numbering.
    pub generic_numbers: bool,
}

/// x86-64. Note `R10` in the fourth slot: the syscall ABI is deliberately not
/// the SysV *call* ABI, which uses `RCX` there, because `syscall` clobbers it.
const X86_64: SyscallAbi = SyscallAbi {
    number: "RAX",
    args: ["RDI", "RSI", "RDX", "R10", "R8", "R9"],
    result: "RAX",
    generic_numbers: false,
};

const AARCH64: SyscallAbi = SyscallAbi {
    number: "x8",
    args: ["x0", "x1", "x2", "x3", "x4", "x5"],
    result: "x0",
    generic_numbers: true,
};

/// ARM 32-bit EABI. `r7` holds the number, and the result comes back in `r0`.
const ARM32: SyscallAbi = SyscallAbi {
    number: "r7",
    args: ["r0", "r1", "r2", "r3", "r4", "r5"],
    result: "r0",
    generic_numbers: true,
};

impl SyscallAbi {
    /// The ABI for an architecture, falling back to x86-64.
    ///
    /// The fallback is not a guess about the architecture -- it is what the
    /// emulator did for every architecture until now, kept so that adding a
    /// name here can only improve things.
    pub fn for_arch(arch: &ArchInfo) -> Self {
        if arch.name.starts_with("AARCH64") {
            AARCH64
        } else if arch.name.starts_with("ARM") {
            ARM32
        } else {
            X86_64
        }
    }

    /// This number, in the numbering the syscall registry is keyed on.
    pub fn canonical_number(&self, number: u64) -> Option<u64> {
        if !self.generic_numbers {
            return Some(number);
        }
        generic_to_x86_64(number)
    }
}

/// asm-generic number → the x86-64 number for the same syscall.
///
/// Only the calls this emulator implements are listed: a translation for a
/// syscall with no handler would turn "unknown syscall 999" into "unknown
/// syscall 42", which is a worse report, not a better one.
fn generic_to_x86_64(number: u64) -> Option<u64> {
    Some(match number {
        29 => 16,   // ioctl
        56 => 257,  // openat
        57 => 3,    // close
        62 => 8,    // lseek
        63 => 0,    // read
        64 => 1,    // write
        66 => 20,   // writev
        79 => 262,  // newfstatat
        80 => 5,    // fstat
        93 => 60,   // exit
        94 => 231,  // exit_group
        96 => 218,  // set_tid_address
        98 => 202,  // futex
        113 => 228, // clock_gettime
        124 => 24,  // sched_yield
        129 => 62,  // kill
        130 => 200, // tkill
        134 => 13,  // rt_sigaction
        135 => 14,  // rt_sigprocmask
        139 => 15,  // rt_sigreturn
        160 => 63,  // uname
        169 => 96,  // gettimeofday
        172 => 39,  // getpid
        174 => 102, // getuid
        175 => 107, // geteuid
        176 => 104, // getgid
        177 => 108, // getegid
        178 => 186, // gettid
        214 => 12,  // brk
        215 => 11,  // munmap
        222 => 9,   // mmap
        226 => 10,  // mprotect
        261 => 302, // prlimit64
        278 => 318, // getrandom
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_architecture_gets_its_own_registers() {
        assert_eq!(SyscallAbi::for_arch(&ArchInfo::x86_64_sysv()).number, "RAX");
        assert_eq!(SyscallAbi::for_arch(&ArchInfo::aarch64()).number, "x8");
        assert_eq!(SyscallAbi::for_arch(&ArchInfo::arm32()).number, "r7");
    }

    #[test]
    fn the_generic_table_is_not_the_x86_64_one() {
        let abi = SyscallAbi::for_arch(&ArchInfo::aarch64());
        // `write` is 64 in the generic table and 1 on x86-64. Reading an
        // aarch64 image's 64 as x86-64's 64 would call `sched_setaffinity`.
        assert_eq!(abi.canonical_number(64), Some(1));
        assert_eq!(abi.canonical_number(93), Some(60)); // exit
        assert_eq!(abi.canonical_number(94), Some(231)); // exit_group
    }

    #[test]
    fn a_syscall_with_no_handler_stays_unknown() {
        let abi = SyscallAbi::for_arch(&ArchInfo::aarch64());
        // 999 is not a syscall. Inventing an x86-64 number for it would turn
        // an honest "unknown" into a call to something unrelated.
        assert_eq!(abi.canonical_number(999), None);
    }

    #[test]
    fn x86_64_numbers_pass_through_untouched() {
        let abi = SyscallAbi::for_arch(&ArchInfo::x86_64_sysv());
        for n in [0u64, 1, 60, 231, 999] {
            assert_eq!(abi.canonical_number(n), Some(n));
        }
    }
}
