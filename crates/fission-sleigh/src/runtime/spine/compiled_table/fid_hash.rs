//! Ghidra-compatible FID (Function ID) "full hash" computation.
//!
//! Ports the full-hash half of `MessageDigestFidHasher.hash()`
//! (`Ghidra/Features/FunctionID/.../hash/MessageDigestFidHasher.java`):
//! an FNV-1a 64-bit digest over each code unit's masked bytes (see
//! [`super::instruction_pattern_mask`]) plus a per-operand mixing step.
//!
//! Deliberately **not** implemented yet: the "specific hash" (needs actual
//! scalar operand values and relocation-awareness -- see
//! `MessageDigestFidHasher.java`'s `hasRelocation`/`OperandType.isAddress`
//! handling, which Fission's flat `BoundOperand` model doesn't currently
//! distinguish) and memory-operand mixing (a memory reference like
//! `[rbp+8]` doesn't produce a `BoundOperand` at all in the current
//! `RuntimeConstructState` -- its base register/displacement live only in
//! the p-code that computes the address, which isn't traced back here).
//! Both are silently skipped rather than guessed at, since a wrong operand
//! mix would make the hash wrong in a way nothing could ever detect (unlike
//! an incomplete extent, which just produces `None`).

use super::*;

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
/// `MessageDigestFidHasher.java`'s operand loop). A register operand mixes
/// in its SLEIGH register-space offset (which register matters for
/// identity); a scalar or address operand -- indistinguishable from
/// Fission's `BoundOperand::Immediate` today, but they're mixed identically
/// for the full hash regardless (`fullUpdate += 0xfeeddead` either way in
/// the Java source) -- contributes a fixed placeholder, since only its
/// *presence*, not its value, is part of an instruction's full-hash
/// identity. Returns `None` for operand shapes this doesn't handle yet
/// (memory references -- see module doc comment).
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

        for (operand_index, operand) in state.operands.iter().enumerate() {
            if let Some(update) =
                mix_operand_full(operand_index, operand, resolve_register_offset)
            {
                digest.update_i32(update);
            }
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
