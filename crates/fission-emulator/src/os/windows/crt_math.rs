//! The C math library, answered with the host's own.
//!
//! These take and return `double`s, which the x64 calling convention passes
//! in the XMM registers rather than the integer ones. Answering them the way
//! every other stub answers -- a zero in RAX -- is the "silently wrong"
//! failure: the program reads its result from XMM0, finds whatever the last
//! floating-point operation left there, and carries on. duktape called
//! `trunc` nine times on the way to its first prompt, and every number it
//! formats goes through that path.
//!
//! Positional slots are shared on Win64: the n-th argument is in the n-th
//! integer register *or* the n-th XMM register, never both. So
//! `frexp(double x, int *exp)` has `x` in XMM0 and `exp` in RDX, and
//! `strtod(const char *s, char **end)` has both pointers in RCX and RDX.
//!
//! The answers are the host's `f64` operations, which are IEEE 754 like the
//! guest's. Where a function's exact value is not pinned by IEEE (the
//! transcendental ones), the host libm may differ from the UCRT's in the last
//! bit; that is the same disagreement two real machines have.

use anyhow::Result;

use crate::core::Emulator;

/// The low 64 bits of an XMM register: where a scalar `double` lives.
const XMM: [&str; 4] = ["XMM0_Qa", "XMM1_Qa", "XMM2_Qa", "XMM3_Qa"];

fn double_arg(emu: &mut Emulator, index: usize) -> Result<f64> {
    Ok(f64::from_bits(emu.read_register_u64(XMM[index])?))
}

fn return_double(emu: &mut Emulator, value: f64) -> Result<()> {
    emu.write_register_u64(XMM[0], value.to_bits())
}

/// Answer a math call. Returns whether the name was one.
///
/// A guest with no `XMM0_Qa` -- anything but x86-64 -- is not answered here,
/// so the miss is still reported by name rather than returning a value in a
/// register that does not exist.
pub fn dispatch(emu: &mut Emulator, name: &str) -> Result<bool> {
    if !emu
        .register_map
        .keys()
        .any(|k| k.eq_ignore_ascii_case(XMM[0]))
    {
        return Ok(false);
    }

    let unary: Option<fn(f64) -> f64> = match name {
        "trunc" => Some(f64::trunc),
        "floor" => Some(f64::floor),
        "ceil" => Some(f64::ceil),
        // C's `round` rounds half away from zero, which is Rust's too.
        "round" => Some(f64::round),
        "sqrt" => Some(f64::sqrt),
        "cbrt" => Some(f64::cbrt),
        "exp" => Some(f64::exp),
        "log" => Some(f64::ln),
        "log10" => Some(f64::log10),
        "log2" => Some(f64::log2),
        "sin" => Some(f64::sin),
        "cos" => Some(f64::cos),
        "tan" => Some(f64::tan),
        "asin" => Some(f64::asin),
        "acos" => Some(f64::acos),
        "atan" => Some(f64::atan),
        "sinh" => Some(f64::sinh),
        "cosh" => Some(f64::cosh),
        "tanh" => Some(f64::tanh),
        "fabs" => Some(f64::abs),
        _ => None,
    };
    if let Some(op) = unary {
        let x = double_arg(emu, 0)?;
        return_double(emu, op(x))?;
        return Ok(true);
    }

    let binary: Option<fn(f64, f64) -> f64> = match name {
        "pow" => Some(f64::powf),
        "atan2" => Some(f64::atan2),
        // C's `fmod` keeps the dividend's sign, which is what Rust's `%` on
        // floats does -- not `rem_euclid`.
        "fmod" => Some(|x, y| x % y),
        "hypot" | "_hypot" => Some(f64::hypot),
        "copysign" | "_copysign" => Some(f64::copysign),
        "fmin" => Some(f64::min),
        "fmax" => Some(f64::max),
        _ => None,
    };
    if let Some(op) = binary {
        let x = double_arg(emu, 0)?;
        let y = double_arg(emu, 1)?;
        return_double(emu, op(x, y))?;
        return Ok(true);
    }

    match name {
        "frexp" => frexp(emu)?,
        "strtod" | "_strtod_l" => strtod(emu)?,
        "atof" => {
            let text_at = emu.read_arg(0).unwrap_or(0);
            let text = read_bytes(emu, text_at);
            let (value, _) = parse_c_double(&text);
            return_double(emu, value)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// `frexp(x, &exp)`: `x = mantissa * 2^exp` with the mantissa in [0.5, 1).
fn frexp(emu: &mut Emulator) -> Result<()> {
    let x = double_arg(emu, 0)?;
    let exp_at = emu.read_arg(1).unwrap_or(0);
    let (mantissa, exponent) = frexp_parts(x);
    if exp_at != 0 {
        let space = emu.state.ram_space();
        emu.state
            .write_space(space, exp_at, &exponent.to_le_bytes())?;
    }
    return_double(emu, mantissa)
}

pub fn frexp_parts(x: f64) -> (f64, i32) {
    if x == 0.0 || x.is_nan() || x.is_infinite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7FF) as i32;
    if raw_exponent == 0 {
        // Subnormal: scale into the normal range first, then give the shift
        // back to the exponent.
        let (mantissa, exponent) = frexp_parts(x * 2f64.powi(54));
        return (mantissa, exponent - 54);
    }
    let exponent = raw_exponent - 1022;
    let mantissa_bits = (bits & !(0x7FFu64 << 52)) | (1022u64 << 52);
    (f64::from_bits(mantissa_bits), exponent)
}

/// `strtod(text, &end)`: the value of the longest prefix that is a number,
/// and where that prefix ended. A caller learns "this was not a number" from
/// `end == text`, so the end pointer matters as much as the value.
fn strtod(emu: &mut Emulator) -> Result<()> {
    let text_at = emu.read_arg(0).unwrap_or(0);
    let end_at = emu.read_arg(1).unwrap_or(0);
    let text = read_bytes(emu, text_at);
    let (value, consumed) = parse_c_double(&text);
    if end_at != 0 {
        let ptr_size = u64::from(emu.arch.pointer_size);
        let end = text_at.wrapping_add(consumed as u64);
        let space = emu.state.ram_space();
        emu.state
            .write_space(space, end_at, &end.to_le_bytes()[..ptr_size as usize])?;
    }
    return_double(emu, value)
}

fn read_bytes(emu: &mut Emulator, at: u64) -> Vec<u8> {
    let space = emu.state.ram_space();
    let mut bytes = Vec::new();
    let mut cursor = at;
    while at != 0 && bytes.len() < 512 {
        match emu.state.read_space(space, cursor, 1) {
            Ok(b) if !b.is_empty() && b[0] != 0 => bytes.push(b[0]),
            _ => break,
        }
        cursor += 1;
    }
    bytes
}

/// C's `strtod` grammar over a byte string. Returns the value and how many
/// bytes the number used (zero if there was no number).
pub fn parse_c_double(text: &[u8]) -> (f64, usize) {
    let mut at = 0;
    while at < text.len() && text[at].is_ascii_whitespace() {
        at += 1;
    }
    let start = at;
    let mut negative = false;
    if at < text.len() && (text[at] == b'+' || text[at] == b'-') {
        negative = text[at] == b'-';
        at += 1;
    }
    let signed = |v: f64| if negative { -v } else { v };

    let rest = &text[at..];
    let lower: Vec<u8> = rest.iter().take(8).map(u8::to_ascii_lowercase).collect();
    if lower.starts_with(b"infinity") {
        return (signed(f64::INFINITY), at + 8);
    }
    if lower.starts_with(b"inf") {
        return (signed(f64::INFINITY), at + 3);
    }
    if lower.starts_with(b"nan") {
        return (signed(f64::NAN), at + 3);
    }

    // Hexadecimal floating point: 0x1.8p3.
    if lower.starts_with(b"0x") {
        let mut cursor = at + 2;
        let mut mantissa = 0f64;
        let mut digits = 0;
        let mut exponent = 0i32;
        while cursor < text.len() && text[cursor].is_ascii_hexdigit() {
            mantissa = mantissa * 16.0 + (text[cursor] as char).to_digit(16).unwrap() as f64;
            cursor += 1;
            digits += 1;
        }
        if cursor < text.len() && text[cursor] == b'.' {
            cursor += 1;
            while cursor < text.len() && text[cursor].is_ascii_hexdigit() {
                mantissa = mantissa * 16.0 + (text[cursor] as char).to_digit(16).unwrap() as f64;
                exponent -= 4;
                cursor += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            // "0x" with nothing after it is the number 0, ending at the `x`.
            return (signed(0.0), at + 1);
        }
        if cursor < text.len() && (text[cursor] == b'p' || text[cursor] == b'P') {
            let (power, used) = parse_exponent(&text[cursor + 1..]);
            if used > 0 {
                exponent += power;
                cursor += 1 + used;
            }
        }
        return (signed(mantissa * 2f64.powi(exponent)), cursor);
    }

    // Decimal: digits, an optional fraction, an optional exponent.
    let mut cursor = at;
    let mut digits = 0;
    while cursor < text.len() && text[cursor].is_ascii_digit() {
        cursor += 1;
        digits += 1;
    }
    if cursor < text.len() && text[cursor] == b'.' {
        cursor += 1;
        while cursor < text.len() && text[cursor].is_ascii_digit() {
            cursor += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return (0.0, 0);
    }
    let mantissa_end = cursor;
    if cursor < text.len() && (text[cursor] == b'e' || text[cursor] == b'E') {
        let (_, used) = parse_exponent(&text[cursor + 1..]);
        if used > 0 {
            cursor += 1 + used;
        }
    }
    let literal = std::str::from_utf8(&text[start..cursor]).unwrap_or("0");
    let value = literal
        .parse::<f64>()
        .or_else(|_| {
            // "5." is C but not Rust; the exponent-less mantissa always parses
            // once a trailing dot is given a zero.
            let mut fixed = String::from_utf8_lossy(&text[start..mantissa_end]).into_owned();
            fixed.push('0');
            fixed.push_str(std::str::from_utf8(&text[mantissa_end..cursor]).unwrap_or(""));
            fixed.parse::<f64>()
        })
        .unwrap_or(0.0);
    (value, cursor)
}

/// An exponent: optional sign, at least one digit. Returns the value and the
/// bytes used, or zero bytes if there was no exponent (so `1e` stops at `1`).
fn parse_exponent(text: &[u8]) -> (i32, usize) {
    let mut at = 0;
    let mut negative = false;
    if at < text.len() && (text[at] == b'+' || text[at] == b'-') {
        negative = text[at] == b'-';
        at += 1;
    }
    let digits_start = at;
    let mut value: i32 = 0;
    while at < text.len() && text[at].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add((text[at] - b'0') as i32);
        at += 1;
    }
    if at == digits_start {
        return (0, 0);
    }
    (if negative { -value } else { value }, at)
}
