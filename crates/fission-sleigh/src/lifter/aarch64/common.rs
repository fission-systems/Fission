use fission_pcode::Varnode;

pub(super) use super::super::backend::common::{
    const_u64, sign_extend, RAM_SPACE_ID, UNIQUE_SPACE_ID,
};

pub(super) const A64_REG_BASE: u64 = 0xA640_0000;
pub(super) const A64_NZCV_BASE: u64 = 0xA64F_0000;

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
