//! Host-side IEEE float helpers for JIT callouts.
//!
//! Sizes 4 and 8 use native f32/f64. Size 10 (x87 extended) is approximated
//! via f64 (same policy as the offline evaluator). No softfloat dependency.

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

pub fn float_binop(op: u32, size: u32, a_bits: u64, b_bits: u64) -> u64 {
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

fn sign_extend_u64(val: u64, size: u32) -> i64 {
    if size >= 8 {
        return val as i64;
    }
    let bits = (size * 8) as i32;
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
}
