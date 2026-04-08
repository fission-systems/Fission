use fission_pcode::Varnode;

pub(in super::super) const UNIQUE_SPACE_ID: u64 = 3;
pub(in super::super) const RAM_SPACE_ID: u64 = 2;

pub(in super::super) fn const_u64(val: u64, size: u32) -> Varnode {
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

pub(in super::super) fn sign_extend(val: i64, bits: u32) -> i64 {
    let shift = 64u32.saturating_sub(bits);
    (val << shift) >> shift
}
