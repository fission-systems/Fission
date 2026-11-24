//! `CALLOTHER`s whose operands are vectors or SIMD operations.
//!
//! SLEIGH lifts most SIMD instructions to a `CALLOTHER` rather than to p-code:
//! `Rd.16B = NEON_umaxp(Rn.16B, Rm.16B, 1:1)`. The emulator's ordinary userop
//! interface passes `u64` values and loses varnode byte widths, so vector and
//! SIMD `CALLOTHER`s (including narrow 64-bit / scalar forms that share those
//! semantics) are answered here directly from the `PcodeOp` itself, reading and
//! writing whole varnodes. Both engines route to [`answer_vector_userop`], so
//! the JIT and interpreter cannot disagree.

mod aarch64;
mod x86;

use std::collections::BTreeMap;

use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};

use crate::core::Emulator;

/// Is this a `CALLOTHER` with an operand too wide for the `u64` interface?
///
/// Width rather than name: the compiler decides whether to hand an op back
/// here, and it has no userop table to resolve names with unless supplied via
/// [`is_known_vector_userop`].
pub fn is_wide_userop(op: &PcodeOp) -> bool {
    op.opcode == PcodeOpcode::CallOther
        && (op.output.as_ref().is_some_and(|v| v.size > 8) || op.inputs.iter().any(|v| v.size > 8))
}

/// Is `name` a SIMD/vector `CALLOTHER` implemented by this module?
///
/// Includes narrow forms (`pavgb`, `pavgw`, MMX 8-byte instructions, AArch64
/// `VPR64`/scalar `NEON_*` ops) so the interpreter and JIT can route them
/// through [`answer_vector_userop`] with full varnode sizes intact.
pub fn is_known_vector_userop(name: &str) -> bool {
    x86::is_known(name) || aarch64::is_known(name)
}

/// Should `op` be routed through [`answer_vector_userop`] in the interpreter?
pub fn is_vector_userop(op: &PcodeOp, userop_map: &BTreeMap<u32, String>) -> bool {
    if is_wide_userop(op) {
        return true;
    }
    op.opcode == PcodeOpcode::CallOther
        && op
            .inputs
            .first()
            .and_then(|vn| userop_map.get(&(vn.constant_val as u32)))
            .is_some_and(|name| is_known_vector_userop(name))
}

/// Answer a vector/SIMD `CALLOTHER`. Returns whether it was one this knows.
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

    if name.starts_with("NEON_") {
        aarch64::answer(emu, &name, op, &out)
    } else {
        x86::answer(emu, &name, op, &out)
    }
}

pub(super) fn read_bytes(
    emu: &mut Emulator,
    op: &PcodeOp,
    index: usize,
) -> anyhow::Result<Vec<u8>> {
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

pub(super) fn read_const_u64(op: &PcodeOp, index: usize) -> Option<u64> {
    let vn = op.inputs.get(index).filter(|vn| vn.is_constant)?;
    let bits = vn.size.saturating_mul(8);
    let raw = vn.constant_val as u64;
    if bits == 0 || bits >= 64 {
        Some(raw)
    } else {
        Some(raw & ((1u64 << bits) - 1))
    }
}

pub(super) fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let n = bytes.len().min(8);
    buf[..n].copy_from_slice(&bytes[..n]);
    u64::from_le_bytes(buf)
}

pub(super) fn read_elem_u64(v: &[u8], idx: usize, esize: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf[..esize].copy_from_slice(&v[idx * esize..(idx + 1) * esize]);
    u64::from_le_bytes(buf)
}

pub(super) fn sign_extend_esize(val: u64, esize: usize) -> i64 {
    let bits = esize * 8;
    if bits >= 64 {
        val as i64
    } else {
        let shift = 64 - bits;
        ((val << shift) as i64) >> shift
    }
}

pub(super) fn write_elem_u64(out: &mut [u8], idx: usize, esize: usize, val: u64) {
    out[idx * esize..(idx + 1) * esize].copy_from_slice(&val.to_le_bytes()[..esize]);
}

pub(super) fn write_bytes(emu: &mut Emulator, vn: &Varnode, data: &[u8]) -> bool {
    let size = (vn.size as usize).min(data.len());
    emu.state
        .write_space(vn.space_id, vn.offset, &data[..size])
        .is_ok()
}

pub(super) fn write_bytes_exact(emu: &mut Emulator, vn: &Varnode, data: &[u8]) -> bool {
    if data.len() != vn.size as usize {
        return false;
    }
    write_bytes(emu, vn, data)
}
