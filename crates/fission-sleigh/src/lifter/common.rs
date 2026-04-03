use fission_pcode::Varnode;

pub(super) const UNIQUE_SPACE_ID: u64 = 3;
pub(super) const RAM_SPACE_ID: u64 = 2;
pub(super) const A64_REG_BASE: u64 = 0xA640_0000;

#[derive(Debug, Clone)]
pub(super) struct A64TempFactory {
    next: u64,
}

impl A64TempFactory {
    pub(super) fn new(address: u64) -> Self {
        Self {
            next: 0xC000_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6)),
        }
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
