use fission_pcode::Varnode;

pub(super) const UNIQUE_SPACE_ID: u64 = 3;
pub(super) const RAM_SPACE_ID: u64 = 2;
pub(super) const A64_REG_BASE: u64 = 0xA640_0000;
pub(super) const A64_NZCV_BASE: u64 = 0xA64F_0000;
pub(super) const X86_REG_BASE: u64 = 0xA860_0000;
pub(super) const X86_XMM_BASE: u64 = 0xA868_0000;
pub(super) const X86_EFLAGS_BASE: u64 = 0xA86F_0000;

#[derive(Debug, Clone)]
pub(super) struct A64TempFactory {
    next: u64,
}

impl A64TempFactory {
    #[cfg(test)]
    pub(super) fn base_for_address(address: u64) -> u64 {
        0xC000_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6))
    }

    #[cfg(test)]
    pub(super) fn new(address: u64) -> Self {
        Self {
            next: Self::base_for_address(address),
        }
    }

    pub(super) fn with_base(base: u64) -> Self {
        Self { next: base }
    }

    pub(super) fn alloc(&mut self, size: u32) -> Varnode {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: self.next,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next = self.next.wrapping_add(8);
        vn
    }
}

pub(super) fn a64_reg(reg: u32, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: A64_REG_BASE + (u64::from(reg) * 8),
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn a64_flag(off: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: A64_NZCV_BASE + off,
        size: 1,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn a64_flag_n() -> Varnode {
    a64_flag(0)
}

pub(super) fn a64_flag_z() -> Varnode {
    a64_flag(1)
}

pub(super) fn a64_flag_c() -> Varnode {
    a64_flag(2)
}

pub(super) fn a64_flag_v() -> Varnode {
    a64_flag(3)
}

fn x86_flag(bit: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_EFLAGS_BASE + bit,
        size: 1,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn x86_flag_cf() -> Varnode {
    x86_flag(0)
}

pub(super) fn x86_flag_pf() -> Varnode {
    x86_flag(2)
}

pub(super) fn x86_flag_zf() -> Varnode {
    x86_flag(6)
}

pub(super) fn x86_flag_sf() -> Varnode {
    x86_flag(7)
}

pub(super) fn x86_flag_df() -> Varnode {
    x86_flag(10)
}

pub(super) fn x86_flag_of() -> Varnode {
    x86_flag(11)
}

pub(super) fn x86_reg(reg: u32, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_REG_BASE + (u64::from(reg) * 8),
        size,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn x86_xmm_reg(reg: u32, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_XMM_BASE + (u64::from(reg) * 16),
        size,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn const_u64(val: u64, size: u32) -> Varnode {
    let masked = if size >= 8 {
        val
    } else {
        let bits = size.saturating_mul(8);
        if bits == 0 {
            0
        } else {
            val & ((1u64 << bits) - 1)
        }
    };
    let signed = i64::from_ne_bytes(masked.to_ne_bytes());
    Varnode::constant(signed, size)
}

pub(super) fn sign_extend(val: i64, bits: u32) -> i64 {
    let shift = 64u32.saturating_sub(bits);
    (val << shift) >> shift
}
pub(super) const X86_SEG_BASE: u64 = 0xA86A_0000;
pub(super) fn x86_seg(reg: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_SEG_BASE + (u64::from(reg) * 8),
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
}
