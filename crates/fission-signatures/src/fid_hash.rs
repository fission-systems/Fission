//! Ghidra-compatible Function ID (FID) hash computation.
//!
//! Implements the dual FNV-1a 64-bit hash used by Ghidra's Function ID analyser to
//! identify library functions by their instruction byte patterns.
//!
//! ## Algorithm overview (from Ghidra `MessageDigestFidHasher.java`)
//!
//! For each instruction in the function body:
//! - **Full hash** (`full_hash`): FNV-1a over the instruction's bytes, with
//!   branch-target / call-target immediates zeroed out.  This makes the hash
//!   position-independent and tolerant of relocation differences.
//! - **Specific hash** (`specific_hash`): Same base as full hash, but with small
//!   scalar operand values (≤ 0xFFFF) also mixed in.  This gives a finer-grained
//!   fingerprint that captures constant usage patterns.
//!
//! ## Minimum size requirement
//!
//! Functions with fewer than `MIN_CODE_UNITS` (4) decoded instructions return `None`.
//!
//! ## Notes on exact Ghidra compatibility
//!
//! Ghidra applies additional ISA-specific instruction masks (e.g. via
//! `InstructionSkipper` lists).  Our implementation uses `iced-x86` to decode
//! instructions and produce a compatible—though not byte-for-byte identical—hash.
//! Hashes computed here can be stored and matched against our own FID databases;
//! they will not match hashes in `.fidbf` files built by Ghidra directly.

use iced_x86::{Decoder, DecoderOptions, FlowControl, Instruction, OpKind};

/// Minimum number of decoded instructions required to produce a hash.
pub const MIN_CODE_UNITS: usize = 4;

/// Maximum immediate value that contributes to the specific hash
/// (matches Ghidra's `specificSmallScalar` threshold).
const SPECIFIC_SCALAR_MAX: u64 = 0x0000_FFFF;

/// FNV-1a 64-bit hasher.
#[derive(Clone)]
pub struct FnvHasher64 {
    state: u64,
}

impl FnvHasher64 {
    pub const OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
    pub const PRIME: u64 = 1_099_511_628_211;

    pub fn new() -> Self {
        Self { state: Self::OFFSET_BASIS }
    }

    /// Feed a byte slice into the hash state.
    pub fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }

    /// Feed a single byte.
    #[inline]
    pub fn update_byte(&mut self, byte: u8) {
        self.state ^= u64::from(byte);
        self.state = self.state.wrapping_mul(Self::PRIME);
    }

    /// Feed an arbitrary 64-bit value (little-endian bytes).
    pub fn update_u64(&mut self, v: u64) {
        self.update(&v.to_le_bytes());
    }

    /// Return the current hash value.
    pub fn finish(&self) -> u64 {
        self.state
    }
}

impl Default for FnvHasher64 {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of computing Ghidra-compatible FID hashes for a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FidHashQuad {
    /// Full hash — position-independent instruction opcode hash.
    pub full_hash: u64,
    /// Specific hash — full hash + small constant operands.
    pub specific_hash: u64,
    /// Number of decoded code units (non-call instructions).
    pub code_unit_size: u16,
    /// Count of small-scalar contributions to the specific hash.
    pub specific_count: u8,
}

/// Compute dual FNV-1a FID hashes for raw x86/x64 function bytes.
///
/// Returns `None` when fewer than `MIN_CODE_UNITS` instructions can be decoded.
///
/// # Parameters
/// - `bytes` — raw instruction bytes of the function (may extend past the function
///   end; decoding stops at the first invalid instruction or when bytes are exhausted)
/// - `is_64bit` — if `true`, decode in 64-bit mode; otherwise 32-bit
pub fn compute_fid_hash(bytes: &[u8], is_64bit: bool) -> Option<FidHashQuad> {
    if bytes.is_empty() {
        return None;
    }

    let bitness = if is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, bytes, 0, DecoderOptions::NONE);

    let mut full_hasher = FnvHasher64::new();
    let mut specific_hasher = FnvHasher64::new();
    let mut code_units: usize = 0;
    let mut call_count: usize = 0;
    let mut specific_count: usize = 0;
    let mut instr = Instruction::default();

    while decoder.can_decode() {
        decoder.decode_out(&mut instr);
        if instr.is_invalid() {
            break;
        }

        let start = instr.ip() as usize;
        let len = instr.len();
        let end = (start + len).min(bytes.len());
        let instr_bytes = &bytes[start..end];

        // Determine whether this instruction has a branch/call target that we
        // should zero (position-dependent operand).
        let is_branch_or_call = matches!(
            instr.flow_control(),
            FlowControl::Call
                | FlowControl::IndirectCall
                | FlowControl::UnconditionalBranch
                | FlowControl::ConditionalBranch
                | FlowControl::IndirectBranch
        );

        // ── Build the masked byte buffer for the full hash ─────────────────
        //
        // For branches and calls, zero the last `imm_size` bytes (the target).
        // For all other instructions, hash the raw bytes unchanged.
        let imm_size = if is_branch_or_call {
            immediate_size_bytes(&instr)
        } else {
            0
        };

        // Feed the non-immediate prefix bytes first.
        let body_end = len.saturating_sub(imm_size).min(instr_bytes.len());
        full_hasher.update(&instr_bytes[..body_end]);
        specific_hasher.update(&instr_bytes[..body_end]);

        // Feed zeroed placeholder for branch targets (full hash: zeros; specific: also zeros).
        for _ in 0..imm_size {
            full_hasher.update_byte(0);
            specific_hasher.update_byte(0);
        }

        // ── Mix small scalar immediates into the specific hash ──────────────
        // (Only for non-branch instructions — branch targets are already zeroed.)
        if !is_branch_or_call {
            for op_idx in 0..instr.op_count() {
                let kind = instr.op_kind(op_idx);
                let scalar_val: Option<u64> = match kind {
                    OpKind::Immediate8 => Some(u64::from(instr.immediate8())),
                    OpKind::Immediate8to16 => Some(instr.immediate8to16() as u64),
                    OpKind::Immediate8to32 => Some(instr.immediate8to32() as u64),
                    OpKind::Immediate8to64 => Some(instr.immediate8to64() as u64),
                    OpKind::Immediate16 => Some(u64::from(instr.immediate16())),
                    OpKind::Immediate32 => Some(u64::from(instr.immediate32())),
                    OpKind::Immediate32to64 => Some(instr.immediate32to64() as u64),
                    OpKind::Immediate64 => Some(instr.immediate64()),
                    _ => None,
                };
                if let Some(val) = scalar_val {
                    if val > 0 && val <= SPECIFIC_SCALAR_MAX {
                        specific_hasher.update_u64(val);
                        specific_count += 1;
                    }
                }
            }
        }

        // Count call instructions separately (mirrors Ghidra's fullCount logic).
        if matches!(instr.flow_control(), FlowControl::Call | FlowControl::IndirectCall) {
            call_count += 1;
        }
        code_units += 1;
    }

    if code_units < MIN_CODE_UNITS {
        return None;
    }

    // Ghidra: fullCount = codeUnitIndex - callCount
    let full_count = code_units.saturating_sub(call_count).min(u16::MAX as usize) as u16;

    Some(FidHashQuad {
        full_hash: full_hasher.finish(),
        specific_hash: specific_hasher.finish(),
        code_unit_size: full_count,
        specific_count: specific_count.min(u8::MAX as usize) as u8,
    })
}

/// Return the byte size of the last immediate operand encoded in `instr`.
/// (Used for zeroing branch/call targets in the hash stream.)
fn immediate_size_bytes(instr: &Instruction) -> usize {
    let mut total = 0usize;
    for op_idx in 0..instr.op_count() {
        let bytes = match instr.op_kind(op_idx) {
            OpKind::Immediate8
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64 => 1,
            OpKind::Immediate16 => 2,
            OpKind::Immediate32 | OpKind::Immediate32to64 => 4,
            OpKind::Immediate64 => 8,
            OpKind::NearBranch16 => 1,
            OpKind::NearBranch32 => 4,
            OpKind::NearBranch64 => 4,
            OpKind::FarBranch16 => 2,
            OpKind::FarBranch32 => 4,
            _ => 0,
        };
        total += bytes;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_hasher_empty_is_offset_basis() {
        let h = FnvHasher64::new();
        assert_eq!(h.finish(), FnvHasher64::OFFSET_BASIS);
    }

    #[test]
    fn fnv_hasher_known_single_byte() {
        let mut h = FnvHasher64::new();
        h.update_byte(0x00);
        let expected = FnvHasher64::OFFSET_BASIS.wrapping_mul(FnvHasher64::PRIME);
        assert_eq!(h.finish(), expected);
    }

    #[test]
    fn compute_fid_hash_returns_none_for_empty() {
        assert_eq!(compute_fid_hash(&[], true), None);
    }

    #[test]
    fn compute_fid_hash_returns_none_for_too_short() {
        // Three NOPs — below MIN_CODE_UNITS (4)
        let nops = &[0x90u8, 0x90, 0x90];
        assert_eq!(compute_fid_hash(nops, true), None);
    }

    #[test]
    fn compute_fid_hash_succeeds_for_minimal_function() {
        // Four NOPs — exactly MIN_CODE_UNITS
        let nops = &[0x90u8, 0x90, 0x90, 0x90];
        let quad = compute_fid_hash(nops, true).expect("should hash 4 nops");
        assert_eq!(quad.code_unit_size, 4);
        assert_eq!(quad.specific_count, 0);
    }

    #[test]
    fn compute_fid_hash_is_deterministic() {
        // Simple x64 function: push rbp / mov rbp,rsp / xor eax,eax / pop rbp / ret
        let prologue: &[u8] = &[
            0x55,             // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0x31, 0xC0,       // xor eax, eax
            0x5D,             // pop rbp
            0xC3,             // ret
        ];
        let a = compute_fid_hash(prologue, true);
        let b = compute_fid_hash(prologue, true);
        assert_eq!(a, b, "FID hash must be deterministic");
    }

    #[test]
    fn full_and_specific_hashes_differ_when_immediates_present() {
        // push rbp / mov rbp,rsp / mov eax, 1 / pop rbp / ret
        let with_imm: &[u8] = &[
            0x55,             // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0xB8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1
            0x5D,             // pop rbp
            0xC3,             // ret
        ];
        let quad = compute_fid_hash(with_imm, true).expect("should hash");
        assert!(
            quad.specific_count > 0,
            "small immediate 1 should increment specific_count"
        );
        assert_ne!(
            quad.full_hash, quad.specific_hash,
            "specific hash should differ from full hash when small immediates are present"
        );
    }

    #[test]
    fn hashes_differ_for_different_opcodes() {
        // xor eax,eax vs sub eax,eax — same operands, different opcodes
        let xor_body: &[u8] = &[0x55, 0x48, 0x89, 0xE5, 0x31, 0xC0, 0x5D, 0xC3];
        let sub_body: &[u8] = &[0x55, 0x48, 0x89, 0xE5, 0x2B, 0xC0, 0x5D, 0xC3];
        let q1 = compute_fid_hash(xor_body, true).unwrap();
        let q2 = compute_fid_hash(sub_body, true).unwrap();
        assert_ne!(
            q1.full_hash, q2.full_hash,
            "different opcodes should produce different full hashes"
        );
    }

    #[test]
    fn branch_targets_are_position_independent() {
        // jmp near forward: E9 04 00 00 00 (jump +4) vs E9 10 00 00 00 (jump +16)
        // Both should produce the same full_hash because the target is zeroed.
        let jmp4: &[u8] = &[
            0x55,             // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0xE9, 0x04, 0x00, 0x00, 0x00, // jmp near +4
            0xC3,             // ret
        ];
        let jmp16: &[u8] = &[
            0x55,             // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0xE9, 0x10, 0x00, 0x00, 0x00, // jmp near +16
            0xC3,             // ret
        ];
        let q1 = compute_fid_hash(jmp4, true).unwrap();
        let q2 = compute_fid_hash(jmp16, true).unwrap();
        assert_eq!(
            q1.full_hash, q2.full_hash,
            "full_hash should be identical for same body with different branch offsets"
        );
    }
}
