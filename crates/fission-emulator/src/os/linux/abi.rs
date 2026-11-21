/// What a `stat`-family call reports about a file, before it is laid out for
/// a particular guest.
#[derive(Debug, Default)]
pub struct TargetStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: u64,
    pub st_atime_nsec: u64,
    pub st_mtime: u64,
    pub st_mtime_nsec: u64,
    pub st_ctime: u64,
    pub st_ctime_nsec: u64,
}

/// Which `struct stat` the guest is about to read.
///
/// Linux has one per architecture family, and 32-bit architectures have two
/// (the original and the large-file `stat64`). They differ in size, in field
/// order and in field width, so writing one where another is expected both
/// scrambles what the guest reads and writes past the end of its buffer.
/// Every guest used to get x86-64's 144 bytes.
///
/// Offsets below are from the kernel's UAPI headers (`asm/stat.h` for each
/// target), not from memory of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatLayout {
    /// x86-64 `struct stat`: 144 bytes, `st_nlink` (64-bit) before `st_mode`.
    X86_64,
    /// The generic 64-bit `struct stat` (AArch64 and everything else on
    /// `asm-generic`): 128 bytes, `st_mode` at 16.
    Generic64,
    /// i386 `struct stat64`: 96 bytes, packed -- `st_size` at 44.
    I386Stat64,
    /// ARM EABI `struct stat64`: 104 bytes -- the same fields with 64-bit
    /// members aligned to 8, so `st_size` is at 48.
    ArmStat64,
    /// The original 32-bit `struct stat` of i386 and ARM (`stat`, `lstat`,
    /// `fstat` without the `64`): 64 bytes, 16-bit mode and link count.
    Old32,
}

impl StatLayout {
    /// The layout a `stat`-family *syscall* fills in, given the number the
    /// guest used -- its own architecture's, not the canonical one, since on
    /// a 32-bit guest `fstat` and `fstat64` are different structures behind
    /// what the dispatcher treats as one call.
    pub fn for_syscall(arch: &crate::arch::ArchInfo, raw_number: u64) -> Self {
        // stat / lstat / fstat, which share numbers on i386 and ARM.
        let original = matches!(raw_number, 106..=108);
        if arch.name.starts_with("AARCH64") {
            Self::Generic64
        } else if arch.name.starts_with("ARM") {
            if original {
                Self::Old32
            } else {
                Self::ArmStat64
            }
        } else if arch.pointer_size == 4 {
            if original {
                Self::Old32
            } else {
                Self::I386Stat64
            }
        } else {
            Self::X86_64
        }
    }

    /// The layout of libc's own `struct stat` -- what a guest calling the
    /// `stat` *function* passes. On 32-bit targets that is the large-file
    /// structure (musl has no other, and glibc's `__xstat64` is the same).
    pub fn for_libc(arch: &crate::arch::ArchInfo) -> Self {
        Self::for_syscall(arch, 0)
    }

    pub fn size(self) -> usize {
        match self {
            Self::X86_64 => 144,
            Self::Generic64 => 128,
            Self::I386Stat64 => 96,
            Self::ArmStat64 => 104,
            Self::Old32 => 64,
        }
    }
}

impl TargetStat {
    /// The bytes to write into guest memory for a guest that expects
    /// `layout`: exactly `layout.size()` of them, little-endian.
    pub fn to_bytes(&self, layout: StatLayout) -> Vec<u8> {
        let mut buf = vec![0u8; layout.size()];
        let mut put = |offset: usize, width: usize, value: u64| {
            buf[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
        };
        let size = self.st_size as u64;
        let blksize = self.st_blksize as u64;
        let blocks = self.st_blocks as u64;
        let mode = u64::from(self.st_mode);
        let uid = u64::from(self.st_uid);
        let gid = u64::from(self.st_gid);
        match layout {
            StatLayout::X86_64 => {
                put(0, 8, self.st_dev);
                put(8, 8, self.st_ino);
                put(16, 8, self.st_nlink);
                put(24, 4, mode);
                put(28, 4, uid);
                put(32, 4, gid);
                put(40, 8, self.st_rdev);
                put(48, 8, size);
                put(56, 8, blksize);
                put(64, 8, blocks);
                put(72, 8, self.st_atime);
                put(80, 8, self.st_atime_nsec);
                put(88, 8, self.st_mtime);
                put(96, 8, self.st_mtime_nsec);
                put(104, 8, self.st_ctime);
                put(112, 8, self.st_ctime_nsec);
            }
            StatLayout::Generic64 => {
                put(0, 8, self.st_dev);
                put(8, 8, self.st_ino);
                put(16, 4, mode);
                put(20, 4, self.st_nlink);
                put(24, 4, uid);
                put(28, 4, gid);
                put(32, 8, self.st_rdev);
                put(48, 8, size);
                put(56, 4, blksize);
                put(64, 8, blocks);
                put(72, 8, self.st_atime);
                put(80, 8, self.st_atime_nsec);
                put(88, 8, self.st_mtime);
                put(96, 8, self.st_mtime_nsec);
                put(104, 8, self.st_ctime);
                put(112, 8, self.st_ctime_nsec);
            }
            StatLayout::I386Stat64 | StatLayout::ArmStat64 => {
                // The same members; ARM aligns its 64-bit ones to 8 bytes
                // where i386 packs them, which moves everything from
                // `st_size` on.
                let shift = if layout == StatLayout::ArmStat64 {
                    4
                } else {
                    0
                };
                put(0, 8, self.st_dev);
                put(12, 4, self.st_ino); // __st_ino: the low half
                put(16, 4, mode);
                put(20, 4, self.st_nlink);
                put(24, 4, uid);
                put(28, 4, gid);
                put(32, 8, self.st_rdev);
                put(44 + shift, 8, size);
                put(52 + shift, 4, blksize);
                put(56 + 2 * shift, 8, blocks);
                put(64 + 2 * shift, 4, self.st_atime);
                put(68 + 2 * shift, 4, self.st_atime_nsec);
                put(72 + 2 * shift, 4, self.st_mtime);
                put(76 + 2 * shift, 4, self.st_mtime_nsec);
                put(80 + 2 * shift, 4, self.st_ctime);
                put(84 + 2 * shift, 4, self.st_ctime_nsec);
                put(88 + 2 * shift, 8, self.st_ino);
            }
            StatLayout::Old32 => {
                put(0, 4, self.st_dev);
                put(4, 4, self.st_ino);
                put(8, 2, mode);
                put(10, 2, self.st_nlink);
                put(12, 2, uid);
                put(14, 2, gid);
                put(16, 4, self.st_rdev);
                put(20, 4, size);
                put(24, 4, blksize);
                put(28, 4, blocks);
                put(32, 4, self.st_atime);
                put(36, 4, self.st_atime_nsec);
                put(40, 4, self.st_mtime);
                put(44, 4, self.st_mtime_nsec);
                put(48, 4, self.st_ctime);
                put(52, 4, self.st_ctime_nsec);
            }
        }
        buf
    }
}

/// Bounded libc HLE thread-header contract. This describes ABI storage, not a
/// complete TLS implementation: no DTV or PT_TLS modules are synthesized.
pub(super) struct RuntimeGuardLayout {
    pub thread_pointer_register: &'static str,
    pub header_size: u64,
    pub self_offsets: &'static [u64],
    pub stack_guard_offset: u64,
    pub pointer_guard_offset: u64,
}

impl RuntimeGuardLayout {
    pub fn for_arch(arch: &crate::arch::ArchInfo) -> Option<Self> {
        // Linux x86-64 LP64 libc thread-header ABI. Other ABIs must supply
        // their own layout; a register named FS alone is not enough evidence.
        (arch.name == "x86:LE:64:default" && arch.pointer_size == 8).then_some(Self {
            thread_pointer_register: "FS_OFFSET",
            header_size: 0x38,
            self_offsets: &[0, 0x10],
            stack_guard_offset: 0x28,
            pointer_guard_offset: 0x30,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::ArchInfo;

    fn sample() -> TargetStat {
        TargetStat {
            st_dev: 0x11,
            st_ino: 0x2222_3333_4444,
            st_nlink: 2,
            st_mode: 0x81B4,
            st_uid: 1000,
            st_gid: 1001,
            st_rdev: 0x55,
            st_size: 0x1_0000_0007,
            st_blksize: 4096,
            st_blocks: 9,
            st_atime: 0x6000_0001,
            st_mtime: 0x6000_0002,
            st_ctime: 0x6000_0003,
            ..Default::default()
        }
    }

    fn field(bytes: &[u8], offset: usize, width: usize) -> u64 {
        let mut raw = [0u8; 8];
        raw[..width].copy_from_slice(&bytes[offset..offset + width]);
        u64::from_le_bytes(raw)
    }

    /// Size, and where the fields a guest reads first are, per layout -- the
    /// numbers the kernel headers give for each target.
    #[test]
    fn each_layout_has_its_own_size_and_field_offsets() {
        // (layout, size, st_mode at/width, st_nlink at/width, st_size at/width,
        //  st_blocks at/width, st_mtime at/width)
        #[rustfmt::skip]
        let table = [
            (StatLayout::X86_64,     144, (24, 4), (16, 8), (48, 8), (64, 8), (88, 8)),
            (StatLayout::Generic64,  128, (16, 4), (20, 4), (48, 8), (64, 8), (88, 8)),
            (StatLayout::I386Stat64,  96, (16, 4), (20, 4), (44, 8), (56, 8), (72, 4)),
            (StatLayout::ArmStat64,  104, (16, 4), (20, 4), (48, 8), (64, 8), (80, 4)),
            (StatLayout::Old32,       64, ( 8, 2), (10, 2), (20, 4), (28, 4), (40, 4)),
        ];
        let stat = sample();
        for (layout, size, mode, nlink, st_size, blocks, mtime) in table {
            let bytes = stat.to_bytes(layout);
            assert_eq!(bytes.len(), size, "{layout:?}");
            assert_eq!(layout.size(), size, "{layout:?}");
            assert_eq!(field(&bytes, mode.0, mode.1), 0x81B4, "{layout:?} st_mode");
            assert_eq!(field(&bytes, nlink.0, nlink.1), 2, "{layout:?} st_nlink");
            let mask = if st_size.1 == 8 {
                u64::MAX
            } else {
                0xFFFF_FFFF
            };
            assert_eq!(
                field(&bytes, st_size.0, st_size.1),
                0x1_0000_0007 & mask,
                "{layout:?} st_size"
            );
            assert_eq!(field(&bytes, blocks.0, blocks.1), 9, "{layout:?} st_blocks");
            assert_eq!(
                field(&bytes, mtime.0, mtime.1),
                0x6000_0002,
                "{layout:?} st_mtime"
            );
        }
        // The large-file layouts carry the whole inode number at their end.
        let i386 = stat.to_bytes(StatLayout::I386Stat64);
        assert_eq!(field(&i386, 88, 8), 0x2222_3333_4444);
        let arm = stat.to_bytes(StatLayout::ArmStat64);
        assert_eq!(field(&arm, 96, 8), 0x2222_3333_4444);
    }

    /// Which layout a call gets: by architecture, and on 32-bit ones by
    /// whether it is the original call or its large-file successor.
    #[test]
    fn the_layout_follows_the_architecture_and_the_call() {
        let x64 = ArchInfo::x86_64_sysv();
        let a64 = ArchInfo::aarch64();
        let x86 = ArchInfo::x86_32();
        let arm = ArchInfo::arm32();
        assert_eq!(StatLayout::for_syscall(&x64, 5), StatLayout::X86_64);
        assert_eq!(StatLayout::for_syscall(&a64, 80), StatLayout::Generic64);
        assert_eq!(StatLayout::for_syscall(&x86, 197), StatLayout::I386Stat64);
        assert_eq!(StatLayout::for_syscall(&x86, 300), StatLayout::I386Stat64);
        assert_eq!(StatLayout::for_syscall(&x86, 108), StatLayout::Old32);
        assert_eq!(StatLayout::for_syscall(&arm, 197), StatLayout::ArmStat64);
        assert_eq!(StatLayout::for_syscall(&arm, 327), StatLayout::ArmStat64);
        assert_eq!(StatLayout::for_syscall(&arm, 108), StatLayout::Old32);
        assert_eq!(StatLayout::for_libc(&x86), StatLayout::I386Stat64);
        assert_eq!(StatLayout::for_libc(&a64), StatLayout::Generic64);
    }
}
