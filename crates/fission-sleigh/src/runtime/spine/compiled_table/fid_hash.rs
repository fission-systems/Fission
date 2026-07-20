//! Ghidra-compatible FID (Function ID) full *and* specific hash computation.
//!
//! Ports `MessageDigestFidHasher.hash()`
//! (`Ghidra/Features/FunctionID/.../hash/MessageDigestFidHasher.java`): an
//! FNV-1a 64-bit digest, computed twice in one pass (full and specific),
//! over each code unit's masked bytes (see [`super::instruction_pattern_mask`])
//! plus a per-operand mixing step, including simple memory operands
//! (`[reg]`, `[reg+disp]`) and SIB addressing.
//!
//! RIP-relative operands (memory loads *and* `LEA`) need no special
//! handling: Fission's runtime resolves them to a literal target address at
//! decode time (`BoundOperand::Immediate`). For the *full* hash,
//! `MessageDigestFidHasher.java` mixes `Address` and `Scalar` objects
//! identically (`fullUpdate += 0xfeeddead` either way). For the *specific*
//! hash they diverge -- but not the way the object type alone would
//! suggest: empirically (a headless Ghidra script printing
//! `OperandType.isAddress`/`getOperandType` for several real instructions --
//! see `fid_full_hash_matches_ghidra_exactly_for_rip_relative_memory_load`/
//! `_lea` and the specific-hash tests below), a dereferenced RIP-relative
//! memory *load* is `isAddress=true` (placeholder), while `LEA`'s computed
//! value is `isAddress=false` (real value used) even though both are
//! "addresses" in a colloquial sense -- LEA computes a value, it doesn't
//! dereference one. The signal that actually matches Ghidra's classification
//! is `RuntimeFixedHandle::space == "ram"`: true for memory dereferences
//! *and*, surprisingly, direct `CALL`/`JMP` targets (both resolve through
//! the code/ram address space), false for `LEA` and plain immediates (both
//! land in `"const"` space). See `mix_operand`'s doc comment for the full
//! reasoning and `tests.rs` for the Ghidra cross-checks.
//!
//! Not relocation-aware yet (`MessageDigestFidHasher.java`'s
//! `hasRelocation` check, which forces the placeholder regardless of the
//! above when an operand's bytes carry a relocation) -- see
//! `compute_fid_hashes`'s doc comment for the concrete, bounded impact of
//! that gap.
//!
//! A wrong operand mix would make the hash wrong in a way nothing could
//! ever detect (unlike an incomplete extent, which just produces `None`),
//! so unhandled memory-address shapes (address computed through an op
//! `trace_simple_memory_address` doesn't recognize) fail the whole
//! function's hash rather than silently mixing a placeholder for something
//! that isn't actually a scalar or omitting an operand Ghidra wouldn't.

use super::*;
use crate::compiler::CompiledDisplayPiece;

/// Ghidra's `getOpObjects(ii)` returns multiple sub-objects for one display
/// operand (e.g. a memory reference is `[Register(base), Scalar(disp)]`, or
/// for SIB addressing `[Register(base), Register(index), Scalar(scale),
/// Scalar(disp)]` -- cross-checked against real Ghidra 12.0.4, which prints
/// a `Scalar(scale)` object even when `scale == 1` and omits the
/// displacement `Scalar` entirely when `disp == 0`, see
/// `fid_full_hash_matches_ghidra_exactly_for_sib_addressing` below); this is
/// the address-shape Fission recovers for the "ram"-space handles that
/// `RuntimeConstructState` doesn't otherwise carry a `BoundOperand` for, by
/// tracing backward through this instruction's own p-code from the handle's
/// `(offset_space, offset_offset, offset_size)` triple to whichever earlier
/// op computed it.
enum MemoryAddressShape {
    /// `[reg]` -- SLEIGH register-space offset of the base register.
    Register(u64),
    /// `[reg+disp]` / `[reg-disp]` -- base register offset, signed displacement.
    RegisterPlusConstant(u64, i64),
    /// `[base+index*scale]` / `[base+index*scale+disp]` -- SIB addressing.
    /// The scale and displacement values are tracked (not just presence) for
    /// the specific hash, which mixes small (`-256 < v < 256`) compound-operand
    /// scalars by their real value -- see `mix_memory_operand`.
    BaseIndexScale {
        base: u64,
        index: u64,
        displacement: Option<i64>,
        scale: i64,
    },
}

/// If `varnode` is itself a register (not produced by any op -- e.g. a raw
/// input to this instruction's p-code), returns its register-space offset.
fn as_register_offset(v: &fission_pcode::Varnode, register_space_index: u64) -> Option<u64> {
    (!v.is_constant && u64::from(v.space_id) == register_space_index).then_some(v.offset)
}

/// If `varnode` was computed by an `IntAdd(register, constant)` op earlier
/// in `ops` (either input order), returns the register's offset and the
/// constant. Used both directly (`[reg+disp]`) and as a sub-match inside SIB
/// base+disp folding.
fn producer_reg_plus_const(
    ops: &[fission_pcode::PcodeOp],
    v: &fission_pcode::Varnode,
    register_space_index: u64,
) -> Option<(u64, i64)> {
    let producer = ops
        .iter()
        .find(|op| op.output.as_ref().is_some_and(|out| out == v))?;
    if producer.opcode != fission_pcode::PcodeOpcode::IntAdd {
        return None;
    }
    let [a, b] = producer.inputs.as_slice() else {
        return None;
    };
    if let Some(offset) = as_register_offset(a, register_space_index) {
        if b.is_constant {
            return Some((offset, b.constant_val as i64));
        }
    }
    if let Some(offset) = as_register_offset(b, register_space_index) {
        if a.is_constant {
            return Some((offset, a.constant_val as i64));
        }
    }
    None
}

/// If `varnode` was computed by an `IntMult(register, constant)` op earlier
/// in `ops` (either input order) -- SLEIGH's encoding of `index*scale` --
/// returns the index register's offset and the scale.
fn producer_scaled_index(
    ops: &[fission_pcode::PcodeOp],
    v: &fission_pcode::Varnode,
    register_space_index: u64,
) -> Option<(u64, i64)> {
    let producer = ops
        .iter()
        .find(|op| op.output.as_ref().is_some_and(|out| out == v))?;
    if producer.opcode != fission_pcode::PcodeOpcode::IntMult {
        return None;
    }
    let [a, b] = producer.inputs.as_slice() else {
        return None;
    };
    if let Some(offset) = as_register_offset(a, register_space_index) {
        if b.is_constant {
            return Some((offset, b.constant_val as i64));
        }
    }
    if let Some(offset) = as_register_offset(b, register_space_index) {
        if a.is_constant {
            return Some((offset, a.constant_val as i64));
        }
    }
    None
}

/// Trace a memory operand's address computation backward through `ops`
/// (this instruction's own p-code) to recover a [`MemoryAddressShape`].
/// `target` identifies the address-holding varnode via the owning handle's
/// `RuntimeFixedHandle::{offset_space, offset_offset, offset_size}`.
///
/// Handles a bare register, register+constant `IntAdd` (simple displacement),
/// and SIB addressing (`base + index*scale` optionally folded with a
/// displacement, observed as either `IntAdd(base,disp) -> IntMult(index,scale)
/// -> IntAdd(combine)` when `disp != 0`, or directly `IntMult(index,scale) ->
/// IntAdd(base,combine)` when `disp == 0` -- both cross-checked against
/// Fission's own p-code output for real SIB instructions). Only reached for
/// handles the runtime didn't already resolve to a `BoundOperand` directly
/// (RIP-relative addressing, for instance, never reaches this function --
/// see the module doc comment). Returns `None` for anything else rather
/// than guess.
fn trace_simple_memory_address(
    ops: &[fission_pcode::PcodeOp],
    register_space_index: u64,
    target_space_id: u64,
    target_offset: u64,
    target_size: u32,
) -> Option<MemoryAddressShape> {
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
            // Simple `[reg+disp]`: reg + const directly.
            if let Some(offset) = as_register_offset(a, register_space_index) {
                if b.is_constant {
                    return Some(MemoryAddressShape::RegisterPlusConstant(
                        offset,
                        b.constant_val as i64,
                    ));
                }
            }
            if let Some(offset) = as_register_offset(b, register_space_index) {
                if a.is_constant {
                    return Some(MemoryAddressShape::RegisterPlusConstant(
                        offset,
                        a.constant_val as i64,
                    ));
                }
            }
            // SIB: one side is `index*scale` (an IntMult producer); the
            // other is either a bare base register (no displacement) or
            // itself an `IntAdd(base,disp)` producer (folded displacement).
            for (base_side, index_side) in [(a, b), (b, a)] {
                let Some((index_offset, scale)) =
                    producer_scaled_index(ops, index_side, register_space_index)
                else {
                    continue;
                };
                if let Some(base_offset) = as_register_offset(base_side, register_space_index) {
                    return Some(MemoryAddressShape::BaseIndexScale {
                        base: base_offset,
                        index: index_offset,
                        displacement: None,
                        scale,
                    });
                }
                if let Some((base_offset, disp)) =
                    producer_reg_plus_const(ops, base_side, register_space_index)
                {
                    return Some(MemoryAddressShape::BaseIndexScale {
                        base: base_offset,
                        index: index_offset,
                        displacement: Some(disp),
                        scale,
                    });
                }
            }
            None
        }
        fission_pcode::PcodeOpcode::Copy => {
            let input = producer.inputs.first()?;
            as_register_offset(input, register_space_index).map(MemoryAddressShape::Register)
        }
        _ => None,
    }
}

/// One display operand's contribution to both digests, plus whether it
/// counted toward `specificAdditionalSize` (mirrors `MessageDigestFidHasher.
/// java`'s `specificCount` -- incremented once per `Scalar` sub-object whose
/// *real* value was mixed in, not once per operand).
struct OperandContribution {
    full: i32,
    specific: i32,
    specific_count: u32,
}

const SCALAR_PLACEHOLDER: i32 = 0xfeed_dead_u32 as i32;

fn reg_mix(offset: u64) -> Option<i32> {
    let offset = i32::try_from(offset).ok()?;
    Some(offset.wrapping_add(7_654_321).wrapping_mul(98_777))
}

/// Mixes one `Scalar` sub-object of a *compound* operand (a memory
/// reference's displacement or SIB scale -- never the whole display
/// operand). Ghidra's `else` branch (not `OperandType.isScalar`): the real
/// value is used only when `-256 < val < 256` (scale is always 1/2/4/8, so
/// always counts; displacement often doesn't). No relocation-awareness yet
/// (see module doc comment) -- deliberately not attempted rather than
/// guessed.
fn mix_compound_scalar(val: i64) -> (i32, bool) {
    let counted = (-256..256).contains(&val);
    let used = if counted {
        val
    } else {
        SCALAR_PLACEHOLDER as i64
    };
    let term = (used as i32).wrapping_add(1_234_567).wrapping_mul(67_999);
    (term, counted)
}

/// Mixing for a memory operand's object list (`[Register(base)]`,
/// `[Register(base), Scalar(disp)]`, or SIB's
/// `[Register(base), Register(index), Scalar(scale), Scalar(disp)?]`) --
/// every object accumulates into *one* `fullUpdate`/`specificUpdate` pair
/// before a single digest update each, mirroring `MessageDigestFidHasher.
/// java`'s per-operand loop (the outer `for (Object obj : opObjects)`
/// accumulates before `fullDigest.update(fullUpdate)`, not once per object).
/// A `Scalar` object always contributes the same flat `0xfeeddead`
/// placeholder to the *full* hash regardless of its actual value
/// (`fullUpdate += 0xfeeddead` in the Java source, unconditionally) -- so
/// SIB's scale-always-present + disp-if-nonzero shape can contribute the
/// placeholder once or twice there. The *specific* hash instead mixes each
/// `Scalar` sub-object's real value via [`mix_compound_scalar`] when small
/// enough.
fn mix_memory_operand(
    operand_index: usize,
    shape: &MemoryAddressShape,
) -> Option<OperandContribution> {
    let index_term = i32::try_from(operand_index)
        .ok()?
        .wrapping_add(1)
        .wrapping_mul(7777);
    let mut full = index_term;
    let mut specific = index_term;
    let mut specific_count = 0u32;
    match shape {
        MemoryAddressShape::Register(offset) => {
            let reg = reg_mix(*offset)?;
            full = full.wrapping_add(reg);
            specific = specific.wrapping_add(reg);
        }
        MemoryAddressShape::RegisterPlusConstant(offset, disp) => {
            let reg = reg_mix(*offset)?;
            full = full.wrapping_add(reg);
            specific = specific.wrapping_add(reg);
            full = full.wrapping_add(SCALAR_PLACEHOLDER);
            let (term, counted) = mix_compound_scalar(*disp);
            specific = specific.wrapping_add(term);
            specific_count += u32::from(counted);
        }
        MemoryAddressShape::BaseIndexScale {
            base,
            index,
            displacement,
            scale,
        } => {
            let base_reg = reg_mix(*base)?;
            let index_reg = reg_mix(*index)?;
            full = full.wrapping_add(base_reg).wrapping_add(index_reg);
            specific = specific.wrapping_add(base_reg).wrapping_add(index_reg);
            // The scale factor is always present as its own Scalar object
            // for SIB addressing, even when scale == 1.
            full = full.wrapping_add(SCALAR_PLACEHOLDER);
            let (scale_term, scale_counted) = mix_compound_scalar(*scale);
            specific = specific.wrapping_add(scale_term);
            specific_count += u32::from(scale_counted);
            if let Some(disp) = displacement {
                full = full.wrapping_add(SCALAR_PLACEHOLDER);
                let (disp_term, disp_counted) = mix_compound_scalar(*disp);
                specific = specific.wrapping_add(disp_term);
                specific_count += u32::from(disp_counted);
            }
        }
    }
    Some(OperandContribution {
        full,
        specific,
        specific_count,
    })
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

/// Per-operand mixing for a display operand that resolved to a
/// `BoundOperand` directly (register or immediate -- memory operands go
/// through [`mix_memory_operand`] instead, since they never get a
/// `BoundOperand` at this level).
///
/// A register operand mixes in its SLEIGH register-space offset identically
/// for both digests (`obj instanceof Register` in the Java source).
///
/// An immediate is always the *whole* display operand at this level (SLEIGH
/// never surfaces a memory reference's displacement/scale as a top-level
/// `BoundOperand::Immediate` handle -- those go through
/// `trace_simple_memory_address` instead), matching Ghidra's
/// `OperandType.isScalar(operandType) == true` branch: the real value is
/// used unconditionally (no magnitude cap, unlike the compound-operand
/// branch) *unless* Ghidra would also flag `isAddress` -- which,
/// empirically (`fid_full_hash_matches_ghidra_exactly_for_specific_hash_*`
/// in `tests.rs`), is exactly when this handle's `RuntimeFixedHandle::space`
/// is `"ram"`: true for a dereferenced memory load (`mov eax,[rip+0x100]`)
/// and, surprisingly, a direct `CALL`/`JMP` target too (both resolve
/// through the ram/code address space), but **false** for `LEA` (its
/// computed value lands in `"const"` space, since it's a value, not a
/// dereference) and plain immediates (`mov eax,0x2a`, also `"const"`) --
/// both of which use their real value in the specific hash.
fn mix_operand(
    operand_index: usize,
    operand: &BoundOperand,
    handle_space_is_ram: bool,
    resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
) -> Option<OperandContribution> {
    let index_term = i32::try_from(operand_index)
        .ok()?
        .wrapping_add(1)
        .wrapping_mul(7777);
    match operand {
        BoundOperand::Immediate { value, .. } => {
            let full = index_term.wrapping_add(SCALAR_PLACEHOLDER);
            let (used, counted) = if handle_space_is_ram {
                (SCALAR_PLACEHOLDER as i64, false)
            } else {
                (*value as i64, true)
            };
            let term = (used as i32).wrapping_add(1_234_567).wrapping_mul(67_999);
            Some(OperandContribution {
                full,
                specific: index_term.wrapping_add(term),
                specific_count: u32::from(counted),
            })
        }
        BoundOperand::NamedVarnode { name, .. } => {
            let offset = resolve_register_offset(name)?;
            let mixed = reg_mix(u64::try_from(offset).ok()?)?;
            Some(OperandContribution {
                full: index_term.wrapping_add(mixed),
                specific: index_term.wrapping_add(mixed),
                specific_count: 0,
            })
        }
        // Memory/relative operands: no reliable base-register/displacement
        // decomposition available from RuntimeConstructState today (see
        // module doc comment). Bail rather than mix a wrong value in.
        BoundOperand::Memory { .. }
        | BoundOperand::Relative { .. }
        | BoundOperand::Register { .. } => None,
    }
}

/// The four values `MessageDigestFidHasher.hash()` returns as a
/// `FidHashQuad`: full hash + its code unit count, specific hash + its
/// additional (real-value-used) scalar count.
pub(crate) struct FidHashes {
    pub full_count: u16,
    pub full_hash: u64,
    pub specific_count: u8,
    pub specific_hash: u64,
}

/// Compute Ghidra FID's full *and* specific hashes for a function, given its
/// instruction extent (e.g. `DecodedPcodeFunction.instructions`, which the
/// normal decompile path already computes and currently discards -- see
/// `RuntimeSleighFrontend::lift_raw_pcode_function_with_context_and_memory_context`).
/// Both digests are computed in one pass since they share the same masked
/// bytes and operand traversal, only diverging in per-operand mixing values
/// (see `mix_operand`/`mix_memory_operand`/`mix_compound_scalar`).
///
/// `resolve_register_offset` resolves a `BoundOperand::NamedVarnode`'s
/// Ghidra-style register name (e.g. `"EAX"`) to its SLEIGH register-space
/// offset -- deliberately a caller-supplied callback rather than a direct
/// dependency, since the register model (`fission_pcode::midend::cspec::
/// RegisterModel`) lives in a crate that depends on `fission-sleigh`, not
/// the other way around.
///
/// Not yet relocation-aware (see module doc comment): a genuine immediate or
/// small memory displacement/scale whose bytes happen to carry a relocation
/// will use its real (link-time-specific) value here where Ghidra would use
/// a placeholder, understating how often the specific-hash bonus should
/// apply. This can only cause a real match to be scored more conservatively
/// (missing the +10 bonus it should have gotten) -- `identify_by_hashes`'
/// `force_specific` filter is the one case this could cause an *incorrect
/// rejection* rather than just a lower score, until relocation-awareness is
/// added.
///
/// Returns `None` if the extent has fewer than [`FID_SHORT_CODE_UNIT_LIMIT`]
/// code units after skipping (mirrors Ghidra returning `null` for "function
/// too small").
pub(crate) fn compute_fid_hashes(
    compiled: &CompiledFrontend,
    extent: &[crate::runtime::DecodedInstruction],
    resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
) -> Option<FidHashes> {
    let mut full_digest = Fnv1a64::new();
    let mut specific_digest = Fnv1a64::new();
    let mut code_unit_index: i32 = -1;
    let mut call_count: i32 = 0;
    let mut specific_count: i32 = 0;

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
            let handle_space_is_ram = handle.fixed.space.as_ref().is_some_and(|s| s.name == "ram");
            let contribution = if let Some(operand) = &handle.debug_value {
                mix_operand(
                    operand_index,
                    operand,
                    handle_space_is_ram,
                    resolve_register_offset,
                )?
            } else if handle_space_is_ram {
                let shape = trace_simple_memory_address(
                    &decode_and_lift_with_details(compiled, &instr.bytes, instr.address)
                        .ok()?
                        .0,
                    compiled.sla_register_space_index,
                    compiled.sla_unique_space_index,
                    handle.fixed.offset_offset,
                    handle.fixed.offset_size,
                )?;
                mix_memory_operand(operand_index, &shape)?
            } else {
                // An operand shape this doesn't understand -- fail the whole
                // function's hash rather than silently mix a wrong or
                // missing contribution in (see module doc comment).
                return None;
            };
            full_digest.update_i32(contribution.full);
            specific_digest.update_i32(contribution.specific);
            specific_count += i32::try_from(contribution.specific_count).ok()?;
        }

        let mask = instruction_pattern_mask(&state, instr.bytes.len());
        let masked_bytes: Vec<u8> = instr
            .bytes
            .iter()
            .zip(mask.iter())
            .map(|(&b, &m)| b & m)
            .collect();
        full_digest.update_bytes(&masked_bytes);
        specific_digest.update_bytes(&masked_bytes);
    }

    code_unit_index += 1; // now a count, not an index
    if code_unit_index < i32::try_from(FID_SHORT_CODE_UNIT_LIMIT).ok()? {
        return None;
    }
    let full_count = code_unit_index - call_count;
    let full_count = u16::try_from(full_count).ok()?;
    // Ghidra: `Math.min(specificCount, Byte.MAX_VALUE)`.
    let specific_count = u8::try_from(specific_count.clamp(0, i32::from(i8::MAX))).ok()?;
    Some(FidHashes {
        full_count,
        full_hash: full_digest.digest_long(),
        specific_count,
        specific_hash: specific_digest.digest_long(),
    })
}
