//! VEX/AVX prefix parsing and routing.
//!
//! Two VEX prefix encodings are handled:
//!   - 2-byte VEX (0xC5): `C5 {R̄|vvvv̄|L|pp} opcode [ModRM…]`
//!   - 3-byte VEX (0xC4): `C4 {R̄|X̄|B̄|map} {W|vvvv̄|L|pp} opcode [ModRM…]`
//!
//! After extracting pp, map_select, REX bits, L bit and the opcode byte the
//! instruction is re-routed to the existing SSE/3-byte escape decoders using
//! an adjusted `op_idx` that keeps all downstream ModRM offsets correct.
//!
//! L bit: 0 = 128-bit XMM, 1 = 256-bit YMM
//!
//! Offset algebra (decode_modrm_operand reads insn[op_idx+1+1]):
//!   2-byte VEX:  new_op_idx = vex_start + 1  →  opcode at +2, ModRM at +3 ✓
//!   3-byte VEX, map 0F:    new_op_idx = vex_start + 2 →  opcode at +3, ModRM at +4 ✓
//!   3-byte VEX, map 0F38/3A: new_op_idx = vex_start + 1 →  ext3 at +2, ModRM at +3 ✓

use super::*;

/// Builds a synthetic `PrefixState` from VEX-encoded pp/REX bits.
///
/// `pp`:  0=None  1=0x66  2=0xF3  3=0xF2
/// `rex`: raw REX byte assembled from VEX R/X/B/W bits (active-high, same
///        layout as an actual REX prefix byte: 0x4W_RXB).
fn vex_prefix_state(pp: u8, rex: u8) -> PrefixState {
    PrefixState {
        operand_size_override: pp == 1,
        address_size_override: false,
        rex,
        rep_prefix: match pp {
            2 => Some(RepPrefix::Rep),
            3 => Some(RepPrefix::Repne),
            _ => None,
        },
        segment_override: None,
    }
}

/// Top-level VEX decoder called when the current opcode is 0xC5 or 0xC4.
///
/// `op_idx` points to the VEX leader byte (0xC5 or 0xC4) inside `insn`.
pub(in super::super) fn decode_vex_semantic(
    insn: &[u8],
    op_idx: usize,
    _outer_prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let leader = insn[op_idx];

    if leader == 0xC5 {
        // ── 2-byte VEX ──────────────────────────────────────────────────────
        // Byte layout: C5 | vex_b2 | opcode | [ModRM …]
        let vex_b2 = match insn.get(op_idx + 1) {
            Some(v) => *v,
            None => return Vec::new(),
        };
        let opcode = match insn.get(op_idx + 2) {
            Some(v) => *v,
            None => return Vec::new(),
        };

        let pp = vex_b2 & 0x03;
        // REX.R is active-low in the VEX byte; convert to REX-style active-high.
        let rex_r_bit: u8 = if (vex_b2 & 0x80) == 0 { 0x04 } else { 0 };
        let rex = 0x40 | rex_r_bit;
        // L bit: 0=128-bit XMM, 1=256-bit YMM
        let vex_l = (vex_b2 & 0x04) != 0;

        let synth = vex_prefix_state(pp, rex);
        // Adjusted op_idx so that decode_modrm_operand(insn, new+1, …) reads
        // insn[op_idx+3] as ModRM.
        let new_op_idx = op_idx + 1;
        if vex_l {
            simd::decode_simd_semantic_avx(insn, new_op_idx, &synth, size, address, temp, seq, opcode)
        } else {
            simd::decode_simd_semantic(insn, new_op_idx, &synth, size, address, temp, seq, opcode)
        }
    } else {
        // ── 3-byte VEX ──────────────────────────────────────────────────────
        // Byte layout: C4 | vex_b2 | vex_b3 | opcode | [ModRM …]
        let vex_b2 = match insn.get(op_idx + 1) {
            Some(v) => *v,
            None => return Vec::new(),
        };
        let vex_b3 = match insn.get(op_idx + 2) {
            Some(v) => *v,
            None => return Vec::new(),
        };
        let opcode = match insn.get(op_idx + 3) {
            Some(v) => *v,
            None => return Vec::new(),
        };

        let map_select = vex_b2 & 0x1F;
        let pp = vex_b3 & 0x03;

        // Assemble a synthetic REX byte from the inverted R/X/B bits and W.
        let rex_r_bit: u8 = if (vex_b2 & 0x80) == 0 { 0x04 } else { 0 };
        let rex_x_bit: u8 = if (vex_b2 & 0x40) == 0 { 0x02 } else { 0 };
        let rex_b_bit: u8 = if (vex_b2 & 0x20) == 0 { 0x01 } else { 0 };
        let rex_w_bit: u8 = if (vex_b3 & 0x80) != 0 { 0x08 } else { 0 };
        let rex = 0x40 | rex_w_bit | rex_r_bit | rex_x_bit | rex_b_bit;
        // VVVV is bits [6:3] of vex_b3, stored inverted (active-low)
        let vvvv_reg = u32::from((!vex_b3 >> 3) & 0xF);
        // L bit: 0=128-bit XMM, 1=256-bit YMM
        let vex_l = (vex_b3 & 0x04) != 0;

        let synth = vex_prefix_state(pp, rex);

        match map_select {
            1 => {
                // Map 0x0F: route to SSE/SIMD decoder.
                // new_op_idx+2 = (op_idx+2)+2 = op_idx+4 = ModRM ✓
                let new_op_idx = op_idx + 2;
                if vex_l {
                    simd::decode_simd_semantic_avx(insn, new_op_idx, &synth, size, address, temp, seq, opcode)
                } else {
                    simd::decode_simd_semantic(insn, new_op_idx, &synth, size, address, temp, seq, opcode)
                }
            }
            2 => {
                // Map 0x0F38: 3-byte escape decoder with VVVV.
                // ext3 = insn[new_op_idx+2] = insn[(op_idx+1)+2] = insn[op_idx+3] = opcode ✓
                // ModRM = insn[op_idx+4] ✓
                let new_op_idx = op_idx + 1;
                escape3byte::decode_three_byte_escape_semantic(
                    insn, new_op_idx, &synth, size, address, temp, seq, false, vvvv_reg,
                )
            }
            3 => {
                // Map 0x0F3A: 3-byte escape decoder (imm8 variant) with VVVV.
                let new_op_idx = op_idx + 1;
                escape3byte::decode_three_byte_escape_semantic(
                    insn, new_op_idx, &synth, size, address, temp, seq, true, vvvv_reg,
                )
            }
            _ => Vec::new(), // Reserved/unknown VEX map
        }
    }
}
