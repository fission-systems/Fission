//! Host-side IEEE float helpers for JIT callouts.
//!
//! Sizes 4 and 8 use native f32/f64 by default. Size 10 (x87 extended) is
//! approximated via f64 (same policy as the offline evaluator).
//!
//! Enable feature `softfloat` for the pure-Rust path in [`super::softfloat`]
//! (NaN quieting + deterministic policy; still no QEMU vendor code).

use crate::arch::NanPolicy;

#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum FloatBinOp {
    Add = 0,
    Sub = 1,
    Mul = 2,
    Div = 3,
    Equal = 4,
    NotEqual = 5,
    Less = 6,
    LessEqual = 7,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum FloatUnOp {
    Neg = 0,
    Abs = 1,
    Sqrt = 2,
    Nan = 3,
    Ceil = 4,
    Floor = 5,
    Round = 6,
    Trunc = 7,
    Int2Float = 8,
    Float2Float = 9,
}

fn bits_to_f64(bits: u64, size: u32) -> f64 {
    match size {
        4 => f32::from_bits(bits as u32) as f64,
        8 | 10 => f64::from_bits(bits),
        _ => f64::from_bits(bits),
    }
}

fn f64_to_bits(val: f64, size: u32) -> u64 {
    match size {
        4 => (val as f32).to_bits() as u64,
        8 | 10 => val.to_bits(),
        _ => val.to_bits(),
    }
}

/// True when `bits` encodes an IEEE 754 NaN of byte width `size` (2, 4, or 8).
pub(crate) fn is_nan_bits(bits: u64, size: u32) -> bool {
    match size {
        2 => (bits & 0x7c00) == 0x7c00 && (bits & 0x03ff) != 0,
        4 => (bits & 0x7f80_0000) == 0x7f80_0000 && (bits & 0x007f_ffff) != 0,
        8 => {
            (bits & 0x7ff0_0000_0000_0000) == 0x7ff0_0000_0000_0000
                && (bits & 0x000f_ffff_ffff_ffff) != 0
        }
        _ => false,
    }
}

/// True when `bits` encodes an IEEE 754 signaling NaN of byte width `size`.
pub(crate) fn is_snan_bits(bits: u64, size: u32) -> bool {
    if !is_nan_bits(bits, size) {
        return false;
    }
    match size {
        2 => (bits & 0x0200) == 0,
        4 => (bits & 0x0040_0000) == 0,
        8 => (bits & 0x0008_0000_0000_0000) == 0,
        _ => false,
    }
}

/// Set the quiet NaN bit on `bits` while preserving sign and payload.
pub(crate) fn quiet_nan_bits(bits: u64, size: u32) -> u64 {
    match size {
        2 => (bits | 0x0200) & 0xffff,
        4 => (bits | 0x0040_0000) & 0xffff_ffff,
        8 => bits | 0x0008_0000_0000_0000,
        _ => bits,
    }
}

/// Pick and quiet the propagated NaN result for a binary float operation
/// according to the guest architecture's [`NanPolicy`].
///
/// Inspected on raw bits before any `f32 -> f64` widening, which on hardware
/// quiets a signaling `f32` NaN and erases the sNaN/qNaN distinction.
pub(crate) fn propagate_nan_bits(
    a_bits: u64,
    b_bits: u64,
    size: u32,
    policy: NanPolicy,
) -> Option<u64> {
    match policy {
        NanPolicy::FirstOperand => {
            if is_nan_bits(a_bits, size) {
                Some(quiet_nan_bits(a_bits, size))
            } else if is_nan_bits(b_bits, size) {
                Some(quiet_nan_bits(b_bits, size))
            } else {
                None
            }
        }
        NanPolicy::SignalingFirst => {
            if is_snan_bits(a_bits, size) {
                Some(quiet_nan_bits(a_bits, size))
            } else if is_snan_bits(b_bits, size) {
                Some(quiet_nan_bits(b_bits, size))
            } else if is_nan_bits(a_bits, size) {
                Some(quiet_nan_bits(a_bits, size))
            } else if is_nan_bits(b_bits, size) {
                Some(quiet_nan_bits(b_bits, size))
            } else {
                None
            }
        }
    }
}

pub fn float_binop(op: u32, size: u32, a_bits: u64, b_bits: u64) -> u64 {
    float_binop_with_policy(op, size, NanPolicy::FirstOperand, a_bits, b_bits)
}

pub fn float_binop_with_policy(
    op: u32,
    size: u32,
    policy: NanPolicy,
    a_bits: u64,
    b_bits: u64,
) -> u64 {
    if cfg!(feature = "softfloat") {
        return crate::jit::softfloat::soft_binop_with_policy(op, size, policy, a_bits, b_bits);
    }
    let is_arith = op == FloatBinOp::Add as u32
        || op == FloatBinOp::Sub as u32
        || op == FloatBinOp::Mul as u32
        || op == FloatBinOp::Div as u32;
    if is_arith {
        if let Some(nan) = propagate_nan_bits(a_bits, b_bits, size, policy) {
            return nan;
        }
    }
    if size == 4 {
        let a = f32::from_bits(a_bits as u32);
        let b = f32::from_bits(b_bits as u32);
        return match op {
            x if x == FloatBinOp::Add as u32 => (a + b).to_bits() as u64,
            x if x == FloatBinOp::Sub as u32 => (a - b).to_bits() as u64,
            x if x == FloatBinOp::Mul as u32 => (a * b).to_bits() as u64,
            x if x == FloatBinOp::Div as u32 => (a / b).to_bits() as u64,
            x if x == FloatBinOp::Equal as u32 => u64::from(a == b),
            x if x == FloatBinOp::NotEqual as u32 => u64::from(a != b),
            x if x == FloatBinOp::Less as u32 => u64::from(a < b),
            x if x == FloatBinOp::LessEqual as u32 => u64::from(a <= b),
            _ => {
                tracing::warn!("float_binop: unknown op {op}");
                0
            }
        };
    }
    let a = bits_to_f64(a_bits, size);
    let b = bits_to_f64(b_bits, size);
    match op {
        x if x == FloatBinOp::Add as u32 => f64_to_bits(a + b, size),
        x if x == FloatBinOp::Sub as u32 => f64_to_bits(a - b, size),
        x if x == FloatBinOp::Mul as u32 => f64_to_bits(a * b, size),
        x if x == FloatBinOp::Div as u32 => f64_to_bits(a / b, size),
        x if x == FloatBinOp::Equal as u32 => u64::from(a == b),
        x if x == FloatBinOp::NotEqual as u32 => u64::from(a != b),
        x if x == FloatBinOp::Less as u32 => u64::from(a < b),
        x if x == FloatBinOp::LessEqual as u32 => u64::from(a <= b),
        _ => {
            tracing::warn!("float_binop: unknown op {op}");
            0
        }
    }
}

pub fn float_unop(op: u32, in_size: u32, out_size: u32, a_bits: u64) -> u64 {
    if cfg!(feature = "softfloat") {
        return crate::jit::softfloat::soft_unop(op, in_size, out_size, a_bits);
    }
    match op {
        x if x == FloatUnOp::Neg as u32 => {
            if in_size == out_size && in_size == 4 {
                return (a_bits ^ 0x8000_0000) & 0xffff_ffff;
            }
            if in_size == out_size && in_size == 8 {
                return a_bits ^ 0x8000_0000_0000_0000;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(-a, out_size)
        }
        x if x == FloatUnOp::Abs as u32 => {
            if in_size == out_size && in_size == 4 {
                return a_bits & 0x7fff_ffff;
            }
            if in_size == out_size && in_size == 8 {
                return a_bits & 0x7fff_ffff_ffff_ffff;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.abs(), out_size)
        }
        x if x == FloatUnOp::Sqrt as u32 => {
            if in_size == out_size && is_nan_bits(a_bits, in_size) {
                return quiet_nan_bits(a_bits, out_size);
            }
            if in_size == out_size && in_size == 4 {
                return f32::from_bits(a_bits as u32).sqrt().to_bits() as u64;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.sqrt(), out_size)
        }
        x if x == FloatUnOp::Nan as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            u64::from(a.is_nan())
        }
        x if x == FloatUnOp::Ceil as u32 => {
            if in_size == out_size && is_nan_bits(a_bits, in_size) {
                return quiet_nan_bits(a_bits, out_size);
            }
            if in_size == out_size && in_size == 4 {
                return f32::from_bits(a_bits as u32).ceil().to_bits() as u64;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.ceil(), out_size)
        }
        x if x == FloatUnOp::Floor as u32 => {
            if in_size == out_size && is_nan_bits(a_bits, in_size) {
                return quiet_nan_bits(a_bits, out_size);
            }
            if in_size == out_size && in_size == 4 {
                return f32::from_bits(a_bits as u32).floor().to_bits() as u64;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.floor(), out_size)
        }
        x if x == FloatUnOp::Round as u32 => {
            if in_size == out_size && is_nan_bits(a_bits, in_size) {
                return quiet_nan_bits(a_bits, out_size);
            }
            if in_size == out_size && in_size == 4 {
                return f32::from_bits(a_bits as u32).round().to_bits() as u64;
            }
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a.round(), out_size)
        }
        x if x == FloatUnOp::Trunc as u32 => {
            // float → int trunc toward zero; result bits as integer of out_size
            let a = bits_to_f64(a_bits, in_size);
            let i = a as i64;
            mask_int(i as u64, out_size)
        }
        x if x == FloatUnOp::Int2Float as u32 => {
            // interpret a_bits as signed integer of in_size → float out_size
            let i = sign_extend_u64(a_bits, in_size);
            f64_to_bits(i as f64, out_size)
        }
        x if x == FloatUnOp::Float2Float as u32 => {
            let a = bits_to_f64(a_bits, in_size);
            f64_to_bits(a, out_size)
        }
        _ => {
            tracing::warn!("float_unop: unknown op {op}");
            0
        }
    }
}

fn mask_int(val: u64, size: u32) -> u64 {
    if size >= 8 {
        val
    } else {
        let bits = (size as u64) * 8;
        val & ((1u64 << bits) - 1)
    }
}

pub(crate) fn sign_extend_u64(val: u64, size: u32) -> i64 {
    if size >= 8 || size == 0 {
        return val as i64;
    }
    let bits = (size * 8) as i32;
    let shift = 64 - bits;
    ((val as i64) << shift) >> shift
}

/// Size-aware integer carry / signed overflow / signed borrow flags.
/// `kind`: 0=INT_CARRY, 1=INT_SCARRY, 2=INT_SBORROW. Returns 0/1.
pub fn int_flag_op(kind: u32, size: u32, a: u64, b: u64) -> u64 {
    let size = size.clamp(1, 8);
    let bits = (size * 8) as u32;
    let mask = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let a = a & mask;
    let b = b & mask;
    let result = match kind {
        0 => {
            // unsigned carry of a+b
            let sum = (a as u128) + (b as u128);
            sum > mask as u128
        }
        1 => {
            // signed overflow of a+b
            let sa = sign_extend_n(a, bits) as i128;
            let sb = sign_extend_n(b, bits) as i128;
            let sum = sa + sb;
            let min = -(1i128 << (bits - 1));
            let max = (1i128 << (bits - 1)) - 1;
            sum < min || sum > max
        }
        2 => {
            // signed overflow of a-b
            let sa = sign_extend_n(a, bits) as i128;
            let sb = sign_extend_n(b, bits) as i128;
            let diff = sa - sb;
            let min = -(1i128 << (bits - 1));
            let max = (1i128 << (bits - 1)) - 1;
            diff < min || diff > max
        }
        _ => false,
    };
    u64::from(result)
}

fn sign_extend_n(val: u64, bits: u32) -> i64 {
    if bits >= 64 {
        return val as i64;
    }
    let shift = 64 - bits;
    ((val as i64) << shift) >> shift
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_add() {
        let a = 1.5f32.to_bits() as u64;
        let b = 2.25f32.to_bits() as u64;
        let r = float_binop(FloatBinOp::Add as u32, 4, a, b);
        assert!((f32::from_bits(r as u32) - 3.75).abs() < 1e-6);
    }

    #[test]
    fn f64_mul() {
        let a = 2.0f64.to_bits();
        let b = 3.0f64.to_bits();
        let r = float_binop(FloatBinOp::Mul as u32, 8, a, b);
        assert!((f64::from_bits(r) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn int_carry_u8() {
        assert_eq!(int_flag_op(0, 1, 0xFF, 1), 1);
        assert_eq!(int_flag_op(0, 1, 0x10, 1), 0);
    }

    #[test]
    fn int_scarry_i8() {
        // 0x7F + 1 overflows signed 8-bit
        assert_eq!(int_flag_op(1, 1, 0x7F, 1), 1);
        assert_eq!(int_flag_op(1, 1, 1, 1), 0);
    }

    #[test]
    fn nan_propagation_follows_isa_policy() {
        let qnan1 = 0xffff_ff64u64;
        let qnan2 = 0xffff_ffffu64;
        let snan1 = 0x7fa0_0001u64;
        let snan2 = 0xffac_ffc4u64;
        let finite = 1.0f32.to_bits() as u64;

        // FirstOperand (x86): first NaN wins (quieted), even over a later sNaN.
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Add as u32,
                4,
                NanPolicy::FirstOperand,
                qnan1,
                qnan2
            ),
            qnan1
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Sub as u32,
                4,
                NanPolicy::FirstOperand,
                qnan2,
                qnan1
            ),
            qnan2
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Add as u32,
                4,
                NanPolicy::FirstOperand,
                qnan1,
                snan2
            ),
            qnan1
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Add as u32,
                4,
                NanPolicy::FirstOperand,
                snan1,
                qnan1
            ),
            snan1 | 0x0040_0000
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Mul as u32,
                4,
                NanPolicy::FirstOperand,
                finite,
                snan2
            ),
            snan2 | 0x0040_0000
        );

        // SignalingFirst (AArch64 FPProcessNaNs): sNaN takes priority and is quieted.
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Sub as u32,
                4,
                NanPolicy::SignalingFirst,
                0xffc0_0001,
                snan2
            ),
            0xffec_ffc4
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Add as u32,
                4,
                NanPolicy::SignalingFirst,
                snan1,
                snan2
            ),
            snan1 | 0x0040_0000
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Add as u32,
                4,
                NanPolicy::SignalingFirst,
                qnan1,
                qnan2
            ),
            qnan1
        );

        // Double-precision (8-byte) NaN propagation
        let dq1 = 0x7ff8_0000_0000_0011u64;
        let dq2 = 0x7ff8_0000_0000_0022u64;
        let ds2 = 0x7ff0_0000_0000_0033u64;
        assert_eq!(
            float_binop_with_policy(FloatBinOp::Add as u32, 8, NanPolicy::FirstOperand, dq1, dq2),
            dq1
        );
        assert_eq!(
            float_binop_with_policy(FloatBinOp::Sub as u32, 8, NanPolicy::FirstOperand, dq1, ds2),
            dq1
        );
        assert_eq!(
            float_binop_with_policy(
                FloatBinOp::Sub as u32,
                8,
                NanPolicy::SignalingFirst,
                dq1,
                ds2
            ),
            ds2 | 0x0008_0000_0000_0000
        );

        // Unary Neg/Abs preserve NaN payload; Sqrt quiets sNaN preserving payload.
        assert_eq!(
            float_unop(FloatUnOp::Neg as u32, 4, 4, snan1),
            snan1 ^ 0x8000_0000
        );
        assert_eq!(
            float_unop(FloatUnOp::Abs as u32, 4, 4, snan2),
            snan2 & 0x7fff_ffff
        );
        assert_eq!(
            float_unop(FloatUnOp::Sqrt as u32, 4, 4, snan1),
            snan1 | 0x0040_0000
        );
    }
}
