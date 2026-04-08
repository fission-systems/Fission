use fission_pcode::Varnode;

pub(super) use super::super::backend::common::{
    const_u64, RAM_SPACE_ID, UNIQUE_SPACE_ID,
};

// Canonical layout constants live in fission-pcode so both the lifter and the
// decompiler share a single definition.
pub(super) use fission_pcode::arch::x86::{
    X86_MXCSR_OFFSET, X86_REG_BASE, X86_SEG_BASE, X86_XMM_BASE, X86_YMM_BASE,
};
// X86_EFLAGS_BASE also needs to be visible to the parent `lifter` module.
pub(in super::super) use fission_pcode::arch::x86::X86_EFLAGS_BASE;

#[derive(Debug, Clone)]
pub(super) struct X86TempFactory {
    pub(super) next: u64,
}

impl X86TempFactory {
    #[cfg(test)]
    pub(super) fn base_for_address(address: u64) -> u64 {
        0xE100_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6))
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

pub(super) fn x86_ymm_reg(reg: u32, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_YMM_BASE + (u64::from(reg) * 32),
        size,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn x86_mxcsr() -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_MXCSR_OFFSET,
        size: 4,
        is_constant: false,
        constant_val: 0,
    }
}

pub(in super::super) fn x86_seg(reg: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_SEG_BASE + (u64::from(reg) * 8),
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
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

pub(super) fn x86_flag_af() -> Varnode {
    x86_flag(4)
}

pub(super) fn x86_flag_zf() -> Varnode {
    x86_flag(6)
}

pub(super) fn x86_flag_sf() -> Varnode {
    x86_flag(7)
}

pub(super) fn x86_flag_if() -> Varnode {
    x86_flag(9)
}

pub(super) fn x86_flag_df() -> Varnode {
    x86_flag(10)
}

pub(super) fn x86_flag_of() -> Varnode {
    x86_flag(11)
}
