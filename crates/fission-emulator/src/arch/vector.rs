//! `CALLOTHER`s whose operands are vectors.
//!
//! SLEIGH lifts most SIMD instructions to a `CALLOTHER` rather than to p-code:
//! `Rd.16B = NEON_umaxp(Rn.16B, Rm.16B, 1:1)`. The emulator's userop interface
//! could not express that -- it hands a handler `&[u64]` and takes a `u64`
//! back, so a sixteen-byte operand arrived as its low half and a sixteen-byte
//! result could not be returned at all.
//!
//! So these are answered here instead, from the `PcodeOp` itself, reading and
//! writing whole varnodes. Both engines route to this one function: the
//! compiled path hands a wide `CALLOTHER` back through the same call-out table
//! the 128-bit integer ops use, and the interpreter calls it directly. Two
//! implementations of a vector instruction would be two chances to get it
//! wrong in different ways, which is the thing the differential gate exists to
//! stop.
//!
//! # What is implemented
//!
//! What the corpus reaches, and nothing else. `NEON_umaxp` is glibc's
//! `strlen`: it is the single most-executed unanswered userop in the dev
//! corpus at 332,694 occurrences, and both aarch64 binaries spin on it for
//! ever because an unanswered one returns zero and the search never
//! terminates. The signed forms come with it because they are the same
//! instruction family and the same code.

use fission_pcode::ir::{PcodeOp, PcodeOpcode};

use crate::core::Emulator;

/// Is this a `CALLOTHER` with an operand too wide for the `u64` interface?
///
/// Width rather than name: the compiler decides whether to hand an op back
/// here, and it has no userop table to resolve names with. A narrow
/// `CALLOTHER` still goes the ordinary way.
pub fn is_wide_userop(op: &PcodeOp) -> bool {
    op.opcode == PcodeOpcode::CallOther
        && (op.output.as_ref().is_some_and(|v| v.size > 8) || op.inputs.iter().any(|v| v.size > 8))
}

/// Answer a vector `CALLOTHER`. Returns whether it was one this knows.
pub fn answer_vector_userop(emu: &mut Emulator, op: &PcodeOp) -> bool {
    let Some(id) = op.inputs.first().map(|vn| vn.constant_val as u32) else {
        return false;
    };
    let Some(name) = emu.userop_map.get(&id).cloned() else {
        return false;
    };
    let Some(out) = op.output.clone() else {
        return false;
    };

    let kind = match name.as_str() {
        "NEON_umaxp" => Pairwise::UnsignedMax,
        "NEON_uminp" => Pairwise::UnsignedMin,
        "NEON_smaxp" => Pairwise::SignedMax,
        "NEON_sminp" => Pairwise::SignedMin,
        _ => return false,
    };

    // inputs: [userop id, Rn, Rm, element size in bytes]
    let Some(esize) = op.inputs.get(3).map(|vn| vn.constant_val as usize) else {
        return false;
    };
    if esize == 0 || esize > 8 {
        return false;
    }
    let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
        return false;
    };
    if a.len() != b.len() || a.is_empty() || a.len() % (esize * 2) != 0 {
        return false;
    }
    let result = pairwise_reduce(&a, &b, esize, kind);
    write_bytes(emu, &out, &result)
}

#[derive(Clone, Copy)]
enum Pairwise {
    UnsignedMax,
    UnsignedMin,
    SignedMax,
    SignedMin,
}

/// The `*P` family: adjacent elements of `a` then of `b`, reduced in pairs.
///
/// The result's first half comes from `a`'s pairs and its second half from
/// `b`'s, which is what "pairwise across the concatenation of the two sources"
/// means once the concatenation is written down.
fn pairwise_reduce(a: &[u8], b: &[u8], esize: usize, kind: Pairwise) -> Vec<u8> {
    let n = a.len() / esize;
    let half = n / 2;
    let element = |v: &[u8], i: usize| -> u64 {
        let mut buf = [0u8; 8];
        buf[..esize].copy_from_slice(&v[i * esize..(i + 1) * esize]);
        u64::from_le_bytes(buf)
    };
    let signed = |v: u64| -> i64 {
        let bits = esize * 8;
        if bits >= 64 {
            v as i64
        } else {
            let shift = 64 - bits;
            ((v << shift) as i64) >> shift
        }
    };

    let mut out = vec![0u8; a.len()];
    for i in 0..n {
        let (src, j) = if i < half { (a, i) } else { (b, i - half) };
        let (x, y) = (element(src, 2 * j), element(src, 2 * j + 1));
        let picked = match kind {
            Pairwise::UnsignedMax => x.max(y),
            Pairwise::UnsignedMin => x.min(y),
            Pairwise::SignedMax => signed(x).max(signed(y)) as u64,
            Pairwise::SignedMin => signed(x).min(signed(y)) as u64,
        };
        out[i * esize..(i + 1) * esize].copy_from_slice(&picked.to_le_bytes()[..esize]);
    }
    out
}

fn read_bytes(emu: &mut Emulator, op: &PcodeOp, index: usize) -> anyhow::Result<Vec<u8>> {
    let vn = op
        .inputs
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("userop input {index} missing"))?;
    if vn.is_constant {
        let mut out = vec![0u8; vn.size as usize];
        for (slot, byte) in out.iter_mut().zip((vn.constant_val as u64).to_le_bytes()) {
            *slot = byte;
        }
        return Ok(out);
    }
    emu.state
        .read_space(vn.space_id, vn.offset, vn.size as usize)
}

fn write_bytes(emu: &mut Emulator, vn: &fission_pcode::ir::Varnode, data: &[u8]) -> bool {
    let size = (vn.size as usize).min(data.len());
    emu.state
        .write_space(vn.space_id, vn.offset, &data[..size])
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umaxp_takes_the_first_half_from_the_first_vector() {
        // Two 8-byte vectors of single-byte elements.
        let a: Vec<u8> = vec![1, 9, 3, 7, 5, 5, 8, 2];
        let b: Vec<u8> = vec![0, 4, 6, 6, 2, 1, 9, 9];
        let out = pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMax);
        // First half: a's pairs. Second half: b's.
        assert_eq!(out, vec![9, 7, 5, 8, 4, 6, 2, 9]);
    }

    #[test]
    fn uminp_is_the_same_shape() {
        let a: Vec<u8> = vec![1, 9, 3, 7, 5, 5, 8, 2];
        let b: Vec<u8> = vec![0, 4, 6, 6, 2, 1, 9, 9];
        let out = pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMin);
        assert_eq!(out, vec![1, 3, 5, 2, 0, 6, 1, 9]);
    }

    #[test]
    fn the_comparison_is_unsigned_unless_it_is_not() {
        // 0xFF is 255 unsigned and -1 signed, which is the whole difference.
        let a: Vec<u8> = vec![0xFF, 0x01, 0, 0, 0, 0, 0, 0];
        let b: Vec<u8> = vec![0; 8];
        assert_eq!(
            pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMax)[0],
            0xFF,
            "255 > 1"
        );
        assert_eq!(
            pairwise_reduce(&a, &b, 1, Pairwise::SignedMax)[0],
            0x01,
            "-1 < 1"
        );
    }

    #[test]
    fn wider_elements_keep_their_width() {
        // Four 16-bit elements per vector, little-endian.
        let a: Vec<u8> = vec![0x00, 0x01, 0xFF, 0x00, 0x34, 0x12, 0x00, 0x00];
        let b: Vec<u8> = vec![0; 8];
        let out = pairwise_reduce(&a, &b, 2, Pairwise::UnsignedMax);
        // max(0x0100, 0x00FF) = 0x0100, then max(0x1234, 0x0000) = 0x1234.
        assert_eq!(&out[0..2], &[0x00, 0x01]);
        assert_eq!(&out[2..4], &[0x34, 0x12]);
    }
}
