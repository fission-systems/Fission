//! What a trap stop means, for the `ptrace` backends.
//!
//! A software breakpoint and a single step both end in the same stop
//! (`SIGTRAP`). Telling them apart from the program counter alone is wrong in
//! one direction on every machine whose breakpoint instruction advances the
//! counter: after `INT3` the counter is one past the breakpoint, and so is a
//! thread that single-stepped a one-byte instruction sitting on a breakpoint
//! address, or stepped a `call` whose target is the byte after one. Reading
//! that as a hit moves the counter back and runs an instruction twice.
//!
//! So the stop is classified from two facts: how the thread was last resumed,
//! and where it is. The rule is the same for every ISA; the only
//! machine-specific input is how far the counter stands past a breakpoint
//! when it traps (`0` for `BRK`, `1` for `INT3`).

/// Why a thread stopped with a trap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrapCause {
    /// It executed the breakpoint instruction this debugger put at `address`.
    Breakpoint { address: u64 },
    /// It finished a single step, or trapped for a reason that is not one of
    /// this debugger's breakpoints.
    Step,
}

/// Classify a trap stop of a thread standing at `pc`.
///
/// * `pc_advance`: how far past a breakpoint the counter is when it traps.
/// * `resumed_by_step`: the thread was resumed to execute one instruction.
///   A step that completed is a step wherever it landed: the instruction at
///   the new counter has not run, so a breakpoint there has not been hit.
/// * `has_breakpoint`: whether this debugger has a breakpoint instruction at
///   an address.
pub(crate) fn classify_trap(
    pc: u64,
    pc_advance: u64,
    resumed_by_step: bool,
    has_breakpoint: impl Fn(u64) -> bool,
) -> TrapCause {
    if resumed_by_step {
        return TrapCause::Step;
    }
    match pc.checked_sub(pc_advance) {
        Some(address) if has_breakpoint(address) => TrapCause::Breakpoint { address },
        _ => TrapCause::Step,
    }
}

/// Lay the program's own bytes over what was read from a target that has
/// breakpoint instructions in it.
///
/// `bytes` is what the target returned for `address..`; it may be shorter
/// than what was asked for. `inserted` yields each breakpoint address with the
/// bytes its instruction replaced.
pub(crate) fn overlay_original_bytes<'a>(
    bytes: &mut [u8],
    address: u64,
    inserted: impl IntoIterator<Item = (u64, &'a [u8])>,
) {
    for (breakpoint, original) in inserted {
        for (index, byte) in original.iter().enumerate() {
            let Some(at) = breakpoint.checked_add(index as u64) else {
                break;
            };
            let Some(offset) = at.checked_sub(address) else {
                continue;
            };
            let Ok(offset) = usize::try_from(offset) else {
                break;
            };
            if let Some(slot) = bytes.get_mut(offset) {
                *slot = *byte;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INT3: u64 = 1;
    const BRK: u64 = 0;

    fn at(addresses: &'static [u64]) -> impl Fn(u64) -> bool {
        move |address| addresses.contains(&address)
    }

    #[test]
    fn a_trap_after_a_continue_at_a_breakpoint_is_that_breakpoint() {
        // Positive control for the step cases below.
        assert_eq!(
            classify_trap(0x1001, INT3, false, at(&[0x1000])),
            TrapCause::Breakpoint { address: 0x1000 }
        );
        assert_eq!(
            classify_trap(0x1000, BRK, false, at(&[0x1000])),
            TrapCause::Breakpoint { address: 0x1000 }
        );
    }

    #[test]
    fn a_step_off_a_one_byte_instruction_is_not_a_second_hit() {
        // The breakpoint at 0x1000 sits on a one-byte instruction (`push
        // rbp`). Stepping it leaves the counter at 0x1001 with the breakpoint
        // back in place: exactly what an INT3 trap looks like.
        assert_eq!(
            classify_trap(0x1001, INT3, true, at(&[0x1000])),
            TrapCause::Step
        );
    }

    #[test]
    fn a_step_that_lands_after_a_breakpointed_byte_is_a_step() {
        // `ret` at 0x0fff has a breakpoint; a stepped `call 0x1000` lands on
        // the function that starts right after it.
        assert_eq!(
            classify_trap(0x1000, INT3, true, at(&[0x0fff])),
            TrapCause::Step
        );
    }

    #[test]
    fn a_step_onto_a_breakpoint_is_a_step_on_every_machine() {
        // The instruction under the breakpoint has not run. With a zero
        // advance the counter equals the breakpoint address, which read as a
        // hit; with an advance of one it never did. Same answer for both.
        assert_eq!(
            classify_trap(0x2000, BRK, true, at(&[0x2000])),
            TrapCause::Step
        );
        assert_eq!(
            classify_trap(0x2000, INT3, true, at(&[0x2000])),
            TrapCause::Step
        );
    }

    #[test]
    fn a_trap_that_is_not_ours_is_not_a_breakpoint() {
        assert_eq!(
            classify_trap(0x3001, INT3, false, at(&[0x1000])),
            TrapCause::Step
        );
        // The counter cannot stand before address zero.
        assert_eq!(classify_trap(0, INT3, false, at(&[0])), TrapCause::Step);
    }

    #[test]
    fn original_bytes_replace_the_breakpoint_instructions_in_a_read() {
        let mut bytes = vec![0xAA; 8];
        let original = [1u8, 2, 3, 4];
        overlay_original_bytes(&mut bytes, 0x1000, [(0x1002, &original[..])]);
        assert_eq!(bytes, [0xAA, 0xAA, 1, 2, 3, 4, 0xAA, 0xAA]);
    }

    #[test]
    fn a_breakpoint_that_straddles_the_read_is_overlaid_where_it_overlaps() {
        let original = [1u8, 2, 3, 4];
        // Starts before the read.
        let mut bytes = vec![0xAA; 4];
        overlay_original_bytes(&mut bytes, 0x1000, [(0x0ffe, &original[..])]);
        assert_eq!(bytes, [3, 4, 0xAA, 0xAA]);
        // Runs past its end.
        let mut bytes = vec![0xAA; 4];
        overlay_original_bytes(&mut bytes, 0x1000, [(0x1002, &original[..])]);
        assert_eq!(bytes, [0xAA, 0xAA, 1, 2]);
    }

    #[test]
    fn a_short_read_is_overlaid_without_indexing_past_it() {
        // Sixteen bytes were asked for and two came back; the breakpoint lies
        // in the part that did not. Indexing by the requested size panicked.
        let original = [1u8, 2, 3, 4];
        let mut bytes = vec![0xAA; 2];
        overlay_original_bytes(&mut bytes, 0x1000, [(0x1008, &original[..])]);
        assert_eq!(bytes, [0xAA, 0xAA]);
        // And one at the top of the address space does not overflow.
        let mut bytes = vec![0xAA; 2];
        overlay_original_bytes(&mut bytes, u64::MAX - 1, [(u64::MAX - 1, &original[..])]);
        assert_eq!(bytes, [1, 2]);
    }
}
