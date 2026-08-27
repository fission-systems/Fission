//! Per-address ARM vs Thumb decode-mode heuristic for ambiguous (even,
//! Thumb-bit-normalized) target addresses.
//!
//! Combines two signal sources rather than Ghidra's full-binary constant-
//! propagation approach (`ArmAnalyzer.flowArmThumb`, which derives an
//! *indirect* branch's target mode by tracing what value actually ends up
//! in the branched-to register -- a different problem: resolving where an
//! indirect call goes, not deciding a *known* target's own mode):
//!
//! Ordered from most to least certain:
//! 1. Direct-call registry: if some other already-decoded function in this
//!    binary calls this exact address via a *direct* `BL`/`BLX`, the call
//!    instruction's own encoding (mnemonic + the caller's own mode)
//!    deterministically fixes the target's mode per the ARM ISA's BL/BLX
//!    interworking rules -- no value tracing needed, ARM's spec already
//!    decided it. This is this module's answer to "what does Ghidra's
//!    approach look like for a problem Fission actually has": harvest the
//!    same fact Ghidra's dataflow analysis would derive, from a cheap
//!    direct-call scan instead of full symbolic execution.
//! 2. 4-byte misalignment: ARM instructions are always 4-byte aligned, so an
//!    unaligned address cannot be ARM-mode at all -- deterministic, not a
//!    heuristic.
//! 3. Thumb function-prologue byte patterns: the target's first few bytes
//!    match one of the handful of `push {reglist, lr}` encodings real
//!    toolchains actually emit for a function entry (ported verbatim from
//!    `archinfo`'s little-endian `ArchARM.thumb_prologs`, battle-tested in
//!    angr's `cfg_fast.py`).
//! 4. Whole-binary entry-point Thumb-bit, as a last-resort weak prior when
//!    none of the above applies (this session's earlier fix).

use crate::PcodeFunction;
use fission_pcode::PcodeOpcode;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Direct-call target mode facts harvested from already-decoded functions,
/// keyed by `(binary content hash, target address with the Thumb bit
/// stripped)`. Global and binary-scoped (not batch-scoped) because Fission
/// has no single "batch decode session" object threaded through every
/// caller -- keying on the binary's own content hash keeps facts from one
/// binary from ever leaking into another's lookups within the same process,
/// which is all correctness requires here.
static THUMB_CALL_TARGETS: LazyLock<Mutex<HashMap<(String, u64), bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Scans a just-decoded function's direct calls (`BL`/`BLX` with a constant
/// target) and records each target's ISA mode, derived from the ARM/Thumb
/// BL/BLX interworking rule (not traced -- the instruction encoding alone
/// decides it):
///   ARM   `bl`  -> target ARM   |  ARM   `blx` -> target Thumb
///   Thumb `bl`  -> target Thumb |  Thumb `blx` -> target ARM
/// so later lookups for those same addresses (any order -- this binary may
/// still have many functions left to decode) get a certain answer instead
/// of falling through to the byte-pattern heuristic.
pub(crate) fn record_direct_call_targets(
    binary_hash: &str,
    caller_was_thumb: bool,
    pcode: &PcodeFunction,
) {
    let mut targets: Vec<(u64, bool)> = Vec::new();
    for block in &pcode.blocks {
        for op in &block.ops {
            if op.opcode != PcodeOpcode::Call {
                continue;
            }
            let Some(target_vn) = op.inputs.first() else {
                continue;
            };
            if !target_vn.is_constant {
                continue;
            }
            let target = if target_vn.offset != 0 {
                target_vn.offset
            } else if target_vn.constant_val >= 0 {
                target_vn.constant_val as u64
            } else {
                continue;
            };
            let is_blx = op
                .asm_mnemonic
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case("blx") || m.eq_ignore_ascii_case("blx.w"));
            let is_bl = op
                .asm_mnemonic
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case("bl") || m.eq_ignore_ascii_case("bl.w"));
            if !is_bl && !is_blx {
                continue;
            }
            // BLX always flips mode across the call; BL always preserves it.
            let target_is_thumb = if is_blx {
                !caller_was_thumb
            } else {
                caller_was_thumb
            };
            targets.push((target & !1, target_is_thumb));
        }
    }
    if targets.is_empty() {
        return;
    }
    if let Ok(mut registry) = THUMB_CALL_TARGETS.lock() {
        for (addr, is_thumb) in targets {
            registry.insert((binary_hash.to_string(), addr), is_thumb);
        }
    }
}

fn known_direct_call_target_mode(binary_hash: &str, decode_entry_address: u64) -> Option<bool> {
    THUMB_CALL_TARGETS
        .lock()
        .ok()?
        .get(&(binary_hash.to_string(), decode_entry_address & !1))
        .copied()
}

static THUMB_PROLOG_RE: LazyLock<regex::bytes::Regex> = LazyLock::new(|| {
    regex::bytes::RegexBuilder::new(
        r"(?x)
        ^(?:
            \x2d\xe9\xb0\x41                                       |  # push.w {r4,r5,r7,r8,lr}
            \x2d\xe9\xf0[\x41\x43\x46\x47\x4d\x4f]                 |  # push.w {r4-r7/r9/r10/r11,r8,lr} variants
            \x2d\xe9\xf8[\x43\x46\x4f]                             |  # push.w {r3-r9/r10/r11,lr} variants
            [\x00\x10\x30\x70\xf0][\xb4\xb5][\x80-\x8f\xa3\xa8]\xb0 |  # push {..,lr}; sub sp,sp,#??
            \x80\xb4[\x80-\xff]\xb0                                |  # push {r7}; sub sp,sp,#??
            [\x00-\xff]\xb4\x00\xb5[\x80-\xff]\xb0                 |  # push {r?,r?}; push {lr}; sub sp,sp,#??
            [\x80-\xff]\xb0[\x00-\xff]\x90                         |  # sub sp,sp,#??; str r0,[sp,?]
            [\x00\x08\x10\x30\x38\x70\xf0\xf8]\xb5[\x00-\xff]\x4c\xa5\x44        |  # push {lr..}; ldr r4,[pc,#??]; add sp,r4
            [\x00\x08\x10\x30\x38\x70\xf0\xf8]\xb5[\x03-\x07\x0c-\x0f\x1e-\x1f]\x46 |  # push {lr..}; mov rX,rY
            \x08\xb5\x00[\x22\x23]                                 |  # push {r3,lr}; movs r2/r3,#0
            [\x00-\xff]\x4b[\x00-\xff]\x4a\x7b\x44\x30\xb5         |  # ldr r3,[pc,#??]; ldr r2,[pc,#??]; add r3,pc; push {r4,r5,lr}
            \x38\xb5\x40\xf2\x00\x03\xc0\xf2\x00\x03                  # push {r3,r4,r5,lr}; mov r3,#0
        )
        ",
    )
    .unicode(false)
    .build()
    .expect("thumb prologue pattern set is a fixed, valid regex")
});

/// Whether an ambiguous (even-address) ARM decode target should be tried as
/// Thumb before ARM. `bytes` must start at `decode_entry_address` and hold
/// at least a handful of bytes (short reads are handled gracefully -- no
/// pattern needs more than 10). `binary_hash` scopes the direct-call
/// registry lookup to this binary.
pub(crate) fn should_prefer_thumb_decode(
    decode_entry_address: u64,
    bytes: &[u8],
    binary_entry_point: u64,
    binary_hash: &str,
) -> bool {
    if let Some(is_thumb) = known_direct_call_target_mode(binary_hash, decode_entry_address) {
        return is_thumb;
    }
    if decode_entry_address % 4 != 0 {
        // ARM instructions are always 4-byte aligned; this address cannot
        // be ARM-mode, full stop.
        return true;
    }
    if THUMB_PROLOG_RE.is_match(bytes) {
        return true;
    }
    binary_entry_point & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unaligned_address_is_always_thumb() {
        assert!(should_prefer_thumb_decode(
            0x8000251,
            &[0, 0, 0, 0],
            0,
            "test-bin-align"
        ));
        assert!(should_prefer_thumb_decode(
            0x8000253,
            &[],
            0,
            "test-bin-align"
        ));
    }

    #[test]
    fn matches_simple_thumb_push_lr_prologue() {
        // push {r4,r5,r6,lr}; ldr r4,[pc,#imm]; add sp,r4 (stmt0/1/2 combo pattern)
        let bytes = [0x70u8, 0xb5, 0x00, 0x4c, 0xa5, 0x44];
        assert!(should_prefer_thumb_decode(
            0x8000250,
            &bytes,
            0,
            "test-bin-prolog1"
        ));
    }

    #[test]
    fn matches_thumb2_push_w_prologue() {
        // push.w {r4,r5,r7,r8,lr}
        let bytes = [0x2du8, 0xe9, 0xb0, 0x41];
        assert!(should_prefer_thumb_decode(
            0x8000250,
            &bytes,
            0,
            "test-bin-prolog2"
        ));
    }

    #[test]
    fn falls_back_to_entry_point_parity_when_no_pattern_matches() {
        let bytes = [0xffu8, 0xff, 0xff, 0xff];
        assert!(should_prefer_thumb_decode(
            0x8000250,
            &bytes,
            0x8000001,
            "test-bin-parity"
        ));
        assert!(!should_prefer_thumb_decode(
            0x8000250,
            &bytes,
            0x8000000,
            "test-bin-parity"
        ));
    }

    #[test]
    fn direct_call_registry_takes_priority_over_everything_else() {
        let bytes = [0xffu8, 0xff, 0xff, 0xff]; // matches no Thumb prologue pattern
        let hash = "test-bin-registry";
        // Simulate an ARM-mode caller doing `blx 0x8000250` -- BLX always
        // flips mode, so the target must be recorded as Thumb even though
        // neither the alignment fact nor the prologue bytes say so.
        let pcode = PcodeFunction {
            blocks: vec![fission_pcode::PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![fission_pcode::PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x1000,
                    output: None,
                    inputs: vec![fission_pcode::Varnode::constant(0x8000250, 4)],
                    asm_mnemonic: Some("blx".to_string()),
                }],
            }],
        };
        record_direct_call_targets(hash, /* caller_was_thumb */ false, &pcode);
        assert!(should_prefer_thumb_decode(0x8000250, &bytes, 0, hash));
    }
}
