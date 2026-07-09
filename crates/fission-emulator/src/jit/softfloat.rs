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
        4 => f32::from_bits(bits as u32) as f64,
        8 | 10 => f64::from_bits(bits),
        _ => f64::from_bits(bits),
    }
}

fn f64_to_bits(val: f64, size: u32) -> u64 {
    match size {
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
            match out_size {
                4 => (i as i32 as u32) as u64,
                _ => i as u64,
            }
        }
        x if x == FloatUnOp::Int2Float as u32 => {
            let i = a_bits as i64;
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

    #[test]
    fn soft_add_f32() {
        let a = 1.5f32.to_bits() as u64;
        let b = 2.25f32.to_bits() as u64;
        let r = soft_binop(FloatBinOp::Add as u32, 4, a, b);
        let f = f32::from_bits(r as u32);
        assert!((f - 3.75).abs() < 1e-6);
    }
}
