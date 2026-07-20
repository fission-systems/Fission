//! Ghidra-compatible FID (Function ID) "full hash" computation.
//!
//! Ports the full-hash half of `MessageDigestFidHasher.hash()`
//! (`Ghidra/Features/FunctionID/.../hash/MessageDigestFidHasher.java`):
//! an FNV-1a 64-bit digest over each code unit's masked bytes (see
//! [`super::instruction_pattern_mask`]) plus a per-operand mixing step,
//! now including simple memory operands (`[reg]`, `[reg+disp]`).
//!
//! Deliberately **not** implemented yet: the "specific hash" (needs actual
//! scalar operand values and relocation-awareness -- see
//! `MessageDigestFidHasher.java`'s `hasRelocation`/`OperandType.isAddress`
//! handling, which Fission's flat `BoundOperand` model doesn't currently
//! distinguish) and SIB-style memory operands (`[base+index*scale+disp]`
//! -- `trace_simple_memory_address` bails on anything beyond a bare
//! register or register+constant `IntAdd`). A wrong operand mix would make
//! the hash wrong in a way nothing could ever detect (unlike an incomplete
//! extent, which just produces `None`), so unhandled shapes fail the whole
//! function's hash rather than silently mixing a placeholder for something
//! that isn't actually a scalar or omitting an operand Ghidra wouldn't.

use super::*;
use crate::compiler::CompiledDisplayPiece;

/// Ghidra's `getOpObjects(ii)` returns multiple sub-objects for one display
/// operand (e.g. a memory reference is `[Register(base), Scalar(disp)]`);
/// this is the address-shape Fission recovers for the "ram"-space handles
/// that `RuntimeConstructState` doesn't otherwise carry a `BoundOperand`
/// for, by tracing backward through this instruction's own p-code from the
/// handle's `(offset_space, offset_offset, offset_size)` triple to whichever
/// earlier op computed it.
enum MemoryAddressShape {
    /// `[reg]` -- SLEIGH register-space offset of the base register.
    Register(u64),
    /// `[reg+disp]` / `[reg-disp]` -- base register offset, signed displacement.
    RegisterPlusConstant(u64, i64),
}

/// Trace a memory operand's address computation backward through `ops`
/// (this instruction's own p-code) to recover a [`MemoryAddressShape`].
/// `target` identifies the address-holding varnode via the owning handle's
/// `RuntimeFixedHandle::{offset_space, offset_offset, offset_size}`.
/// Returns `None` for anything beyond a bare register or register+constant
/// `IntAdd` (e.g. SIB addressing with an index register) rather than guess.
fn trace_simple_memory_address(
    ops: &[fission_pcode::PcodeOp],
    register_space_index: u64,
    target_space_id: u64,
    target_offset: u64,
    target_size: u32,
) -> Option<MemoryAddressShape> {
    let is_register = |v: &fission_pcode::Varnode| -> bool {
        !v.is_constant && u64::from(v.space_id) == register_space_index
    };
    let matches_target = |v: &fission_pcode::Varnode| -> bool {
        v.space_id == target_space_id && v.offset == target_offset && v.size == target_size
    };

    let producer = ops
        .iter()
        .find(|op| op.output.as_ref().is_some_and(matches_target))?;

    match producer.opcode {
        fission_pcode::PcodeOpcode::IntAdd => {
            let [a, b] = producer.inputs.as_slice() else {
                return None;
            };
            let (reg, scalar) = if is_register(a) && b.is_constant {
                (a, b)
            } else if is_register(b) && a.is_constant {
                (b, a)
            } else {
                // e.g. base+index (SIB addressing) -- not handled yet.
                return None;
            };
            let displacement = scalar.constant_val as i64;
            Some(MemoryAddressShape::RegisterPlusConstant(
                reg.offset,
                displacement,
            ))
        }
        fission_pcode::PcodeOpcode::Copy => {
            let input = producer.inputs.first()?;
            is_register(input).then(|| MemoryAddressShape::Register(input.offset))
        }
        _ => None,
    }
}

/// Mixing for a memory operand's `[Register(base), Scalar(disp)]` object
/// list -- both objects accumulate into *one* `fullUpdate` before a single
/// digest update, mirroring `MessageDigestFidHasher.java`'s per-operand loop
/// (the outer `for (Object obj : opObjects)` accumulates before
/// `fullDigest.update(fullUpdate)`, not once per object).
fn mix_memory_operand_full(operand_index: usize, shape: &MemoryAddressShape) -> Option<i32> {
    let mut full_update = i32::try_from(operand_index)
        .ok()?
        .wrapping_add(1)
        .wrapping_mul(7777);
    let base_offset = match shape {
        MemoryAddressShape::Register(offset) | MemoryAddressShape::RegisterPlusConstant(offset, _) => {
            *offset
        }
    };
    let base_offset = i32::try_from(base_offset).ok()?;
    let reg_mix = base_offset.wrapping_add(7_654_321).wrapping_mul(98_777);
    full_update = full_update.wrapping_add(reg_mix);
    if matches!(shape, MemoryAddressShape::RegisterPlusConstant(..)) {
        full_update = full_update.wrapping_add(0xfeed_dead_u32 as i32);
    }
    Some(full_update)
}

/// Ghidra's `FidHasher.SHORT_HASH_CODE_UNIT_LIMIT` (`FidService.java`):
/// functions with fewer code units than this are too generic to fingerprint
/// reliably and are never hashed.
pub const FID_SHORT_CODE_UNIT_LIMIT: usize = 4;

/// Port of `generic.hash.FNV1a64MessageDigest` -- byte-for-byte identical
/// update/reset semantics (offset basis, prime, and reset-after-digest).
pub(crate) struct Fnv1a64 {
    value: u64,
}

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 1_099_511_628_211;

    pub(crate) fn new() -> Self {
        Self {
            value: Self::OFFSET_BASIS,
        }
    }

    fn update_byte(&mut self, byte: u8) {
        self.value ^= u64::from(byte);
        self.value = self.value.wrapping_mul(Self::PRIME);
    }

    pub(crate) fn update_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.update_byte(b);
        }
    }

    /// Mirrors `AbstractMessageDigest.update(int)`: big-endian byte order.
    pub(crate) fn update_i32(&mut self, value: i32) {
        self.update_bytes(&value.to_be_bytes());
    }

    pub(crate) fn digest_long(&mut self) -> u64 {
        let result = self.value;
        self.value = Self::OFFSET_BASIS;
        result
    }
}

/// Port of `X86InstructionSkipper`'s byte patterns: instructions that
/// compilers insert for alignment/padding and that would otherwise make two
/// semantically-identical functions hash differently. Excluded from the hash
/// entirely (not even as masked bytes) and not counted toward the code unit
/// total.
const X86_SKIP_PATTERNS: &[&[u8]] = &[
    &[0x90],
    &[0x8b, 0xc0],
    &[0x8b, 0xc9],
    &[0x8b, 0xd2],
    &[0x8b, 0xdb],
    &[0x8b, 0xe4],
    &[0x8b, 0xed],
    &[0x8b, 0xf6],
    &[0x8b, 0xff],
];

fn x86_skip_instruction(bytes: &[u8]) -> bool {
    X86_SKIP_PATTERNS.iter().any(|pattern| *pattern == bytes)
}

/// Per-operand mixing for the *full* hash only (`fullUpdate` in
/// `MessageDigestFidHasher.java`'s operand loop), for a display operand that
/// resolved to a `BoundOperand` directly (register or immediate -- memory
/// operands go through [`mix_memory_operand_full`] instead, since they never
/// get a `BoundOperand` at this level). A register operand mixes in its
/// SLEIGH register-space offset (which register matters for identity); a
/// scalar or address operand -- indistinguishable from Fission's
/// `BoundOperand::Immediate` today, but they're mixed identically for the
/// full hash regardless (`fullUpdate += 0xfeeddead` either way in the Java
/// source) -- contributes a fixed placeholder, since only its *presence*,
/// not its value, is part of an instruction's full-hash identity.
fn mix_operand_full(
    operand_index: usize,
    operand: &BoundOperand,
    resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
) -> Option<i32> {
    let index_term = i32::try_from(operand_index)
        .ok()?
        .wrapping_add(1)
        .wrapping_mul(7777);
    match operand {
        BoundOperand::Immediate { .. } => {
            let placeholder = 0xfeed_dead_u32 as i32;
            Some(index_term.wrapping_add(placeholder))
        }
        BoundOperand::NamedVarnode { name, .. } => {
            let offset = resolve_register_offset(name)?;
            let offset = i32::try_from(offset).ok()?;
            let mixed = offset.wrapping_add(7_654_321).wrapping_mul(98_777);
            Some(index_term.wrapping_add(mixed))
        }
        // Memory/relative operands: no reliable base-register/displacement
        // decomposition available from RuntimeConstructState today (see
        // module doc comment). Bail rather than mix a wrong value in.
        BoundOperand::Memory { .. } | BoundOperand::Relative { .. } | BoundOperand::Register { .. } => {
            None
        }
    }
}

/// Compute Ghidra FID's "full hash" for a function, given its instruction
/// extent (e.g. `DecodedPcodeFunction.instructions`, which the normal
/// decompile path already computes and currently discards -- see
/// `RuntimeSleighFrontend::lift_raw_pcode_function_with_context_and_memory_context`).
///
/// `resolve_register_offset` resolves a `BoundOperand::NamedVarnode`'s
/// Ghidra-style register name (e.g. `"EAX"`) to its SLEIGH register-space
/// offset -- deliberately a caller-supplied callback rather than a direct
/// dependency, since the register model (`fission_pcode::midend::cspec::
/// RegisterModel`) lives in a crate that depends on `fission-sleigh`, not
/// the other way around.
///
/// Returns `None` if the extent has fewer than [`FID_SHORT_CODE_UNIT_LIMIT`]
/// code units after skipping (mirrors Ghidra returning `null` for "function
/// too small"), or `Some((code_unit_count, hash))` mirroring
/// `FidHashQuad`'s `(fullCount, fullHash)` pair.
pub(crate) fn compute_fid_full_hash(
    compiled: &CompiledFrontend,
    extent: &[crate::runtime::DecodedInstruction],
    resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
) -> Option<(u16, u64)> {
    let mut digest = Fnv1a64::new();
    let mut code_unit_index: i32 = -1;
    let mut call_count: i32 = 0;

    // Ghidra caps at Short.MAX_VALUE - 1 code units.
    const SHORT_MAX_VALUE: usize = i16::MAX as usize;
    for instr in extent.iter().take(SHORT_MAX_VALUE - 1) {
        code_unit_index += 1;

        if x86_skip_instruction(&instr.bytes) {
            code_unit_index -= 1;
            continue;
        }

        if instr.flow_kind == crate::runtime::DecodedFlowKind::Call {
            call_count += 1;
        }

        let state = decode_instruction_raw_state(compiled, &instr.bytes, instr.address).ok()?;

        // Ghidra's getNumOperands()/getOpObjects(ii) enumerate *display*
        // operands, e.g. "mov eax,[rbp+8]" has exactly 2 -- not
        // state.handles' own count, which includes internal/hidden operands
        // (a zero-extend wrapper, an address subtable's own inner unique-space
        // handle, ...) that never appear in the display string. The display
        // template's OperandRef sequence is the authoritative order.
        let display_order = state
            .display_template
            .pieces
            .iter()
            .filter_map(|piece| match piece {
                CompiledDisplayPiece::OperandRef(handle_index) => Some(*handle_index),
                CompiledDisplayPiece::Literal(_) => None,
            })
            .collect::<Vec<_>>();

        for (operand_index, &handle_index) in display_order.iter().enumerate() {
            let handle = state
                .handles
                .iter()
                .find(|h| h.operand_index == handle_index)?;
            let update = if let Some(operand) = &handle.debug_value {
                mix_operand_full(operand_index, operand, resolve_register_offset)?
            } else if handle.fixed.space.as_ref().is_some_and(|s| s.name == "ram") {
                let shape = trace_simple_memory_address(
                    &decode_and_lift_with_details(compiled, &instr.bytes, instr.address)
                        .ok()?
                        .0,
                    compiled.sla_register_space_index,
                    compiled.sla_unique_space_index,
                    handle.fixed.offset_offset,
                    handle.fixed.offset_size,
                )?;
                mix_memory_operand_full(operand_index, &shape)?
            } else {
                // An operand shape this doesn't understand -- fail the whole
                // function's hash rather than silently mix a wrong or
                // missing contribution in (see module doc comment).
                return None;
            };
            digest.update_i32(update);
        }

        let mask = instruction_pattern_mask(&state, instr.bytes.len());
        let masked_bytes: Vec<u8> = instr
            .bytes
            .iter()
            .zip(mask.iter())
            .map(|(&b, &m)| b & m)
            .collect();
        digest.update_bytes(&masked_bytes);
    }

    code_unit_index += 1; // now a count, not an index
    if code_unit_index < i32::try_from(FID_SHORT_CODE_UNIT_LIMIT).ok()? {
        return None;
    }
    let full_count = code_unit_index - call_count;
    let full_count = u16::try_from(full_count).ok()?;
    Some((full_count, digest.digest_long()))
}
