//! Optional pure-Rust softfloat helpers (feature `softfloat`).
//!
//! Default JIT float path uses host `f32`/`f64` in [`super::float_ops`]. When the
//! `softfloat` feature is enabled, ops route here for a more deterministic
//! IEEE-ish policy without QEMU/vendor softfloat code:
//! - explicit quieting of signaling NaNs on compare results
//! - size-10 (x87) still approximated as f64 (documented limitation)
//!
//! This is a correctness-oriented scaffold, not a full softfloat library.

#![allow(dead_code)]

fn bits_to_f64(bits: u64, size: u32) -> f64 {
    match size {
        2 => crate::pcode::eval::f16_to_f64(bits as u16),
        4 => f32::from_bits(bits as u32) as f64,
        8 | 10 => f64::from_bits(bits),
        _ => f64::from_bits(bits),
    }
}

fn f64_to_bits(val: f64, size: u32) -> u64 {
    match size {
        2 => u64::from(crate::pcode::eval::f64_to_f16(val)),
        4 => {
            let f = val as f32;
            // Quiet NaN if signaling.
            let bits = f.to_bits();
            if f.is_nan() {
                (bits | 0x0040_0000) as u64
            } else {
                bits as u64
            }
        }
        8 | 10 => {
            let bits = val.to_bits();
            if val.is_nan() {
                bits | 0x0008_0000_0000_0000
            } else {
                bits
            }
        }
        _ => val.to_bits(),
    }
}

pub fn soft_binop(op: u32, size: u32, a_bits: u64, b_bits: u64) -> u64 {
    use super::float_ops::FloatBinOp;
    let a = bits_to_f64(a_bits, size);
    let b = bits_to_f64(b_bits, size);
    match op {
        x if x == FloatBinOp::Add as u32 => f64_to_bits(a + b, size),
        x if x == FloatBinOp::Sub as u32 => f64_to_bits(a - b, size),
        x if x == FloatBinOp::Mul as u32 => f64_to_bits(a * b, size),
        x if x == FloatBinOp::Div as u32 => f64_to_bits(a / b, size),
        // IEEE: NaN compares false for ordered predicates.
        x if x == FloatBinOp::Equal as u32 => u64::from(a == b),
        x if x == FloatBinOp::NotEqual as u32 => u64::from(a != b),
        x if x == FloatBinOp::Less as u32 => u64::from(a < b),
        x if x == FloatBinOp::LessEqual as u32 => u64::from(a <= b),
        _ => 0,
    }
}

pub fn soft_unop(op: u32, in_size: u32, out_size: u32, a_bits: u64) -> u64 {
    use super::float_ops::FloatUnOp;
    match op {
        x if x == FloatUnOp::Neg as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(-a, out_size)
        }
        x if x == FloatUnOp::Abs as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.abs(), out_size)
        }
        x if x == FloatUnOp::Sqrt as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.sqrt(), out_size)
        }
        x if x == FloatUnOp::Nan as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            u64::from(a.is_nan())
        }
        x if x == FloatUnOp::Ceil as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.ceil(), out_size)
        }
        x if x == FloatUnOp::Floor as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.floor(), out_size)
        }
        x if x == FloatUnOp::Round as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.round(), out_size)
        }
        x if x == FloatUnOp::Trunc as u32 => {
            // Truncate float → int bits in out_size.
            let a = bits_to_f64(a_bits, in_size);
            let i = a as i64;
            crate::jit::float_ops::mask_int(i as u64, out_size)
        }
        x if x == FloatUnOp::Int2Float as u32 => {
            // Signed, at the input's own width -- as the default path and the
            // interpreter read it. `as i64` made a four-byte -1 4294967295.
            let i = crate::jit::float_ops::sign_extend_u64(a_bits, in_size);
            f64_to_bits(i as f64, out_size)
        }
        x if x == FloatUnOp::Float2Float as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a, out_size)
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit::float_ops::FloatBinOp;

    /// `INT2FLOAT` reads its input as a signed integer of its own width: a
    /// four-byte -1 is -1.0, not 4294967295.0.
    #[test]
    fn int2float_sign_extends_a_narrow_input() {
        use crate::jit::float_ops::FloatUnOp;
        let r = soft_unop(FloatUnOp::Int2Float as u32, 4, 8, 0xFFFF_FFFF);
        assert_eq!(f64::from_bits(r), -1.0);
        let r = soft_unop(FloatUnOp::Int2Float as u32, 2, 4, 0xFFFE);
        assert_eq!(f32::from_bits(r as u32), -2.0);
        let r = soft_unop(FloatUnOp::Int2Float as u32, 8, 8, 5);
        assert_eq!(f64::from_bits(r), 5.0);
    }

    #[test]
    fn soft_add_f32() {
        let a = 1.5f32.to_bits() as u64;
        let b = 2.25f32.to_bits() as u64;
        let r = soft_binop(FloatBinOp::Add as u32, 4, a, b);
        let f = f32::from_bits(r as u32);
        assert!((f - 3.75).abs() < 1e-6);
    }

    #[test]
    fn soft_f16_ops() {
        use crate::jit::float_ops::FloatUnOp;
        // 1.5 + 2.25 = 3.75 (0x3e00 + 0x4080 = 0x4380)
        assert_eq!(
            soft_binop(FloatBinOp::Add as u32, 2, 0x3e00, 0x4080),
            0x4380
        );
        // 2.0 * -1.5 = -3.0 (0x4000 * 0xbe00 = 0xc200)
        assert_eq!(
            soft_binop(FloatBinOp::Mul as u32, 2, 0x4000, 0xbe00),
            0xc200
        );
        // sNaN quieting on f16 output
        assert_eq!(
            soft_unop(FloatUnOp::Float2Float as u32, 2, 2, 0x7c01),
            0x7e01
        );
        assert_eq!(soft_unop(FloatUnOp::Nan as u32, 2, 1, 0x7c01), 1);
        assert_eq!(
            soft_unop(FloatUnOp::Int2Float as u32, 4, 2, (-2i32 as u32) as u64),
            0xc000
        );
        assert_eq!(
            soft_unop(FloatUnOp::Trunc as u32, 2, 2, 0xc300),
            (-3i16 as u16) as u64
        );
    }
}
