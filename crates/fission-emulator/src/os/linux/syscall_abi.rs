//! What a Linux x86-64 syscall's arguments *mean*.
//!
//! The emulator's metrics counted syscalls by number -- `{158: 1, 218: 1}` --
//! which says a run made calls without saying what it did. This table is the
//! difference between a counter and a report: `arch_prctl(ARCH_SET_FS,
//! 0x1005398)` names a behaviour, `158` names a row.
//!
//! # Scope
//!
//! Numbers here are **x86-64** (`arch/x86/entry/syscalls/syscall_64.tbl`).
//! They are not portable: `open` is 2 here and 5 on i386, and aarch64 has no
//! `open` at all. A second architecture needs a second table, not an edit to
//! this one.
//!
//! It covers more than the emulator implements. A syscall this emulator will
//! answer with `-ENOSYS` is still worth naming in a behaviour report -- "the
//! sample tried to `execve`" is the finding, whether or not the call was
//! served.

/// How to read one argument register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// A plain number, shown signed (counts, offsets, ids).
    Int,
    /// A number worth seeing in hex (addresses, sizes, masks).
    Hex,
    /// A file descriptor: small non-negative, or a negative errno-like value.
    Fd,
    /// An address whose contents are not read.
    Ptr,
    /// A NUL-terminated string in guest memory. **Read at call time**: by the
    /// time a report is rendered the buffer may be gone.
    Str,
    /// A buffer whose length is in argument `.0`. A bounded prefix is kept.
    Buf(usize),
    /// A bitmask or enumerated code, decoded by [`FlagSet`].
    Flags(FlagSet),
    /// Ignored -- the syscall does not use this register.
    Unused,
}

/// Which set of names a [`ArgKind::Flags`] argument draws on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSet {
    /// `mmap`/`mprotect` protection bits.
    Prot,
    /// `mmap` mapping flags.
    MapFlags,
    /// `open`/`openat` flags.
    OpenFlags,
    /// `arch_prctl` subfunction code (an enum, not a mask).
    ArchPrctl,
    /// A signal number (an enum, not a mask).
    Signal,
    /// `clone` flags.
    CloneFlags,
    /// `socket` address family (an enum).
    AddressFamily,
}

pub struct SyscallSpec {
    pub name: &'static str,
    pub args: &'static [ArgKind],
}

const NONE: &[ArgKind] = &[];

/// The spec for a syscall number, or `None` when the number is not in the
/// table -- which a report should show as `syscall_<n>(...)` rather than
/// pretending it knows.
pub fn spec(number: u64) -> Option<&'static SyscallSpec> {
    use ArgKind::*;
    use FlagSet::*;
    Some(match number {
        0 => &SyscallSpec {
            name: "read",
            args: &[Fd, Ptr, Hex],
        },
        1 => &SyscallSpec {
            name: "write",
            args: &[Fd, Buf(2), Hex],
        },
        2 => &SyscallSpec {
            name: "open",
            args: &[Str, Flags(OpenFlags), Hex],
        },
        3 => &SyscallSpec {
            name: "close",
            args: &[Fd],
        },
        4 => &SyscallSpec {
            name: "stat",
            args: &[Str, Ptr],
        },
        5 => &SyscallSpec {
            name: "fstat",
            args: &[Fd, Ptr],
        },
        6 => &SyscallSpec {
            name: "lstat",
            args: &[Str, Ptr],
        },
        8 => &SyscallSpec {
            name: "lseek",
            args: &[Fd, Int, Int],
        },
        9 => &SyscallSpec {
            name: "mmap",
            args: &[Hex, Hex, Flags(Prot), Flags(MapFlags), Fd, Hex],
        },
        10 => &SyscallSpec {
            name: "mprotect",
            args: &[Hex, Hex, Flags(Prot)],
        },
        11 => &SyscallSpec {
            name: "munmap",
            args: &[Hex, Hex],
        },
        12 => &SyscallSpec {
            name: "brk",
            args: &[Hex],
        },
        13 => &SyscallSpec {
            name: "rt_sigaction",
            args: &[Flags(Signal), Ptr, Ptr, Hex],
        },
        14 => &SyscallSpec {
            name: "rt_sigprocmask",
            args: &[Int, Ptr, Ptr, Hex],
        },
        15 => &SyscallSpec {
            name: "rt_sigreturn",
            args: NONE,
        },
        16 => &SyscallSpec {
            name: "ioctl",
            args: &[Fd, Hex, Hex],
        },
        20 => &SyscallSpec {
            name: "writev",
            args: &[Fd, Ptr, Int],
        },
        21 => &SyscallSpec {
            name: "access",
            args: &[Str, Hex],
        },
        22 => &SyscallSpec {
            name: "pipe",
            args: &[Ptr],
        },
        24 => &SyscallSpec {
            name: "sched_yield",
            args: NONE,
        },
        32 => &SyscallSpec {
            name: "dup",
            args: &[Fd],
        },
        33 => &SyscallSpec {
            name: "dup2",
            args: &[Fd, Fd],
        },
        39 => &SyscallSpec {
            name: "getpid",
            args: NONE,
        },
        41 => &SyscallSpec {
            name: "socket",
            args: &[Flags(AddressFamily), Int, Int],
        },
        42 => &SyscallSpec {
            name: "connect",
            args: &[Fd, Ptr, Hex],
        },
        43 => &SyscallSpec {
            name: "accept",
            args: &[Fd, Ptr, Ptr],
        },
        44 => &SyscallSpec {
            name: "sendto",
            args: &[Fd, Buf(2), Hex, Hex, Ptr, Hex],
        },
        45 => &SyscallSpec {
            name: "recvfrom",
            args: &[Fd, Ptr, Hex, Hex, Ptr, Ptr],
        },
        49 => &SyscallSpec {
            name: "bind",
            args: &[Fd, Ptr, Hex],
        },
        50 => &SyscallSpec {
            name: "listen",
            args: &[Fd, Int],
        },
        56 => &SyscallSpec {
            name: "clone",
            args: &[Flags(CloneFlags), Hex, Ptr, Ptr, Hex],
        },
        57 => &SyscallSpec {
            name: "fork",
            args: NONE,
        },
        59 => &SyscallSpec {
            name: "execve",
            args: &[Str, Ptr, Ptr],
        },
        60 => &SyscallSpec {
            name: "exit",
            args: &[Int],
        },
        61 => &SyscallSpec {
            name: "wait4",
            args: &[Int, Ptr, Int, Ptr],
        },
        62 => &SyscallSpec {
            name: "kill",
            args: &[Int, Flags(Signal)],
        },
        63 => &SyscallSpec {
            name: "uname",
            args: &[Ptr],
        },
        72 => &SyscallSpec {
            name: "fcntl",
            args: &[Fd, Int, Hex],
        },
        79 => &SyscallSpec {
            name: "getcwd",
            args: &[Ptr, Hex],
        },
        80 => &SyscallSpec {
            name: "chdir",
            args: &[Str],
        },
        82 => &SyscallSpec {
            name: "rename",
            args: &[Str, Str],
        },
        83 => &SyscallSpec {
            name: "mkdir",
            args: &[Str, Hex],
        },
        87 => &SyscallSpec {
            name: "unlink",
            args: &[Str],
        },
        88 => &SyscallSpec {
            name: "symlink",
            args: &[Str, Str],
        },
        90 => &SyscallSpec {
            name: "chmod",
            args: &[Str, Hex],
        },
        96 => &SyscallSpec {
            name: "gettimeofday",
            args: &[Ptr, Ptr],
        },
        101 => &SyscallSpec {
            name: "ptrace",
            args: &[Int, Int, Hex, Hex],
        },
        102 => &SyscallSpec {
            name: "getuid",
            args: NONE,
        },
        104 => &SyscallSpec {
            name: "getgid",
            args: NONE,
        },
        105 => &SyscallSpec {
            name: "setuid",
            args: &[Int],
        },
        107 => &SyscallSpec {
            name: "geteuid",
            args: NONE,
        },
        108 => &SyscallSpec {
            name: "getegid",
            args: NONE,
        },
        158 => &SyscallSpec {
            name: "arch_prctl",
            args: &[Flags(ArchPrctl), Hex],
        },
        165 => &SyscallSpec {
            name: "mount",
            args: &[Str, Str, Str, Hex, Ptr],
        },
        186 => &SyscallSpec {
            name: "gettid",
            args: NONE,
        },
        200 => &SyscallSpec {
            name: "tkill",
            args: &[Int, Flags(Signal)],
        },
        201 => &SyscallSpec {
            name: "time",
            args: &[Ptr],
        },
        202 => &SyscallSpec {
            name: "futex",
            args: &[Ptr, Int, Hex, Ptr, Ptr, Hex],
        },
        218 => &SyscallSpec {
            name: "set_tid_address",
            args: &[Ptr],
        },
        228 => &SyscallSpec {
            name: "clock_gettime",
            args: &[Int, Ptr],
        },
        231 => &SyscallSpec {
            name: "exit_group",
            args: &[Int],
        },
        257 => &SyscallSpec {
            name: "openat",
            args: &[Fd, Str, Flags(OpenFlags), Hex],
        },
        262 => &SyscallSpec {
            name: "newfstatat",
            args: &[Fd, Str, Ptr, Hex],
        },
        263 => &SyscallSpec {
            name: "unlinkat",
            args: &[Fd, Str, Hex],
        },
        302 => &SyscallSpec {
            name: "prlimit64",
            args: &[Int, Int, Ptr, Ptr],
        },
        318 => &SyscallSpec {
            name: "getrandom",
            args: &[Ptr, Hex, Hex],
        },
        _ => return None,
    })
}

/// Render a flag argument. Bitmask sets list the bits that are set and keep
/// any remainder in hex, so an unknown bit is visible rather than dropped;
/// enumerated sets name the value or fall back to hex.
pub fn decode_flags(set: FlagSet, value: u64) -> String {
    match set {
        FlagSet::Prot => bits(
            value,
            &[(1, "PROT_READ"), (2, "PROT_WRITE"), (4, "PROT_EXEC")],
            "PROT_NONE",
        ),
        FlagSet::MapFlags => bits(
            value,
            &[
                (0x01, "MAP_SHARED"),
                (0x02, "MAP_PRIVATE"),
                (0x10, "MAP_FIXED"),
                (0x20, "MAP_ANONYMOUS"),
                (0x100, "MAP_GROWSDOWN"),
                (0x4000, "MAP_NORESERVE"),
                (0x8000, "MAP_POPULATE"),
                (0x20000, "MAP_STACK"),
            ],
            "0",
        ),
        FlagSet::OpenFlags => {
            // The low two bits are an access *mode*, not flags.
            let mode = match value & 0x3 {
                0 => "O_RDONLY",
                1 => "O_WRONLY",
                2 => "O_RDWR",
                _ => "O_ACCMODE",
            };
            let rest = bits(
                value & !0x3,
                &[
                    (0x40, "O_CREAT"),
                    (0x80, "O_EXCL"),
                    (0x200, "O_TRUNC"),
                    (0x400, "O_APPEND"),
                    (0x800, "O_NONBLOCK"),
                    (0x80000, "O_CLOEXEC"),
                    (0x10000, "O_DIRECTORY"),
                    (0x20000, "O_NOFOLLOW"),
                ],
                "",
            );
            if rest.is_empty() {
                mode.to_string()
            } else {
                format!("{mode}|{rest}")
            }
        }
        FlagSet::CloneFlags => bits(
            value,
            &[
                (0x00000100, "CLONE_VM"),
                (0x00000200, "CLONE_FS"),
                (0x00000400, "CLONE_FILES"),
                (0x00000800, "CLONE_SIGHAND"),
                (0x00002000, "CLONE_PTRACE"),
                (0x00008000, "CLONE_VFORK"),
                (0x00010000, "CLONE_THREAD"),
                (0x00020000, "CLONE_NEWNS"),
                (0x00080000, "CLONE_SYSVSEM"),
                (0x00100000, "CLONE_SETTLS"),
            ],
            "0",
        ),
        FlagSet::ArchPrctl => named(
            value,
            &[
                (0x1001, "ARCH_SET_GS"),
                (0x1002, "ARCH_SET_FS"),
                (0x1003, "ARCH_GET_FS"),
                (0x1004, "ARCH_GET_GS"),
            ],
        ),
        FlagSet::Signal => named(
            value,
            &[
                (1, "SIGHUP"),
                (2, "SIGINT"),
                (4, "SIGILL"),
                (6, "SIGABRT"),
                (8, "SIGFPE"),
                (9, "SIGKILL"),
                (11, "SIGSEGV"),
                (13, "SIGPIPE"),
                (14, "SIGALRM"),
                (15, "SIGTERM"),
                (17, "SIGCHLD"),
                (19, "SIGSTOP"),
            ],
        ),
        FlagSet::AddressFamily => named(
            value,
            &[
                (1, "AF_UNIX"),
                (2, "AF_INET"),
                (10, "AF_INET6"),
                (16, "AF_NETLINK"),
            ],
        ),
    }
}

fn bits(value: u64, table: &[(u64, &str)], zero: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = value;
    for (bit, name) in table {
        if value & bit != 0 {
            parts.push((*name).to_string());
            rest &= !bit;
        }
    }
    if rest != 0 {
        parts.push(format!("0x{rest:X}"));
    }
    if parts.is_empty() {
        zero.to_string()
    } else {
        parts.join("|")
    }
}

fn named(value: u64, table: &[(u64, &str)]) -> String {
    table
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, n)| (*n).to_string())
        .unwrap_or_else(|| format!("0x{value:X}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_the_table_does_not_know_stays_a_number() {
        assert!(spec(0xDEAD).is_none());
        assert_eq!(spec(158).unwrap().name, "arch_prctl");
    }

    #[test]
    fn a_bitmask_keeps_bits_it_cannot_name() {
        // PROT_READ|PROT_WRITE plus an unmodelled bit: the bit must survive
        // into the output, not be silently dropped.
        assert_eq!(decode_flags(FlagSet::Prot, 3), "PROT_READ|PROT_WRITE");
        assert_eq!(
            decode_flags(FlagSet::Prot, 3 | 0x1000),
            "PROT_READ|PROT_WRITE|0x1000"
        );
        assert_eq!(decode_flags(FlagSet::Prot, 0), "PROT_NONE");
    }

    #[test]
    fn open_flags_split_the_access_mode_from_the_rest() {
        assert_eq!(decode_flags(FlagSet::OpenFlags, 0), "O_RDONLY");
        assert_eq!(
            decode_flags(FlagSet::OpenFlags, 1 | 0x40 | 0x200),
            "O_WRONLY|O_CREAT|O_TRUNC"
        );
    }

    #[test]
    fn an_enumerated_code_is_named_or_shown_in_hex() {
        assert_eq!(decode_flags(FlagSet::ArchPrctl, 0x1002), "ARCH_SET_FS");
        assert_eq!(decode_flags(FlagSet::ArchPrctl, 0x9999), "0x9999");
    }
}
