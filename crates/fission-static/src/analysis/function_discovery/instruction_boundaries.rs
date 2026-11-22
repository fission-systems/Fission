use std::collections::HashSet;

use fission_loader::LoadedBinary;
use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedReferenceKind, PackedContextOverride, RuntimeSleighFrontend,
};

use super::targets::{is_target_no_return, normalize_target, same_section};
use super::thumb::{decode_context_for_address, image_decode_context};

fn merge_intervals(mut intervals: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    if intervals.is_empty() {
        return intervals;
    }
    intervals.sort_unstable_by_key(|&(start, _)| start);
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        if start >= end {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

pub(super) struct InstructionCoverage {
    intervals: Vec<(u64, u64)>,
    known_functions: HashSet<u64>,
}

impl InstructionCoverage {
    pub(super) fn build(
        binary: &LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        known_functions: &HashSet<u64>,
    ) -> Self {
        use rayon::prelude::*;
        let boundaries: Vec<(u64, u64)> = known_functions
            .par_iter()
            .flat_map(|&addr| walk_function_boundaries(binary, frontend, addr, known_functions))
            .collect();

        let intervals = merge_intervals(boundaries);

        Self {
            intervals,
            known_functions: known_functions.clone(),
        }
    }

    /// Walk each new start in parallel against the same set of known
    /// functions, then merge the newly covered instruction ranges once.
    pub(super) fn add_function_starts(
        &mut self,
        binary: &LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        addrs: &[u64],
    ) {
        if addrs.is_empty() {
            return;
        }
        self.known_functions.extend(addrs.iter().copied());
        use rayon::prelude::*;
        let known = &self.known_functions;
        let new_boundaries: Vec<(u64, u64)> = addrs
            .par_iter()
            .flat_map(|&addr| walk_function_boundaries(binary, frontend, addr, known))
            .collect();

        let mut all_intervals = std::mem::take(&mut self.intervals);
        all_intervals.extend(new_boundaries);
        self.intervals = merge_intervals(all_intervals);
    }

    pub(super) fn covers(&self, addr: u64) -> bool {
        match self.intervals.binary_search_by(|&(start, end)| {
            if addr < start {
                std::cmp::Ordering::Greater
            } else if addr >= end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
impl InstructionCoverage {
    pub(super) fn from_raw_boundaries(
        boundaries: Vec<(u64, u64)>,
        known_functions: HashSet<u64>,
    ) -> Self {
        let intervals = merge_intervals(boundaries);
        Self {
            intervals,
            known_functions,
        }
    }

    pub(super) fn intervals(&self) -> &[(u64, u64)] {
        &self.intervals
    }
}

/// The actual CFG-following instruction-boundary walk used by both
/// [`InstructionCoverage::build`] and
/// [`InstructionCoverage::add_function_starts`].
pub(super) fn walk_function_boundaries(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
    known_functions: &HashSet<u64>,
) -> Vec<(u64, u64)> {
    let mut boundaries = Vec::new();
    // The mode the function runs in: the image's, or its own when the loader
    // or a pattern said so.
    let mode_context = decode_context_for_address(binary, frontend, addr, None);
    {
        let exec_ranges = super::ranges::executable_ranges(binary);
        let mut visited = HashSet::new();
        let mut worklist = vec![addr];

        while let Some(curr) = worklist.pop() {
            if visited.contains(&curr) {
                continue;
            }
            if curr != addr
                && (known_functions.contains(&curr)
                    || binary.function_addr_index.contains_key(&curr))
            {
                continue;
            }

            let mut ip = curr;
            let mut count = 0;
            while count < 4000 {
                if !visited.insert(ip) {
                    break;
                }
                if !super::ranges::is_in_executable_ranges(ip, &exec_ranges) {
                    break;
                }
                let Some(bytes) = binary.view_bytes(ip, 15) else {
                    break;
                };
                let decoded = match mode_context {
                    Some(context) => {
                        frontend.decode_window_no_pcode_in_context(bytes, ip, 1, Some(context))
                    }
                    None => frontend.decode_window_no_pcode(bytes, ip, 1),
                };
                let Ok(decoded) = decoded else {
                    break;
                };
                if decoded.is_empty() {
                    break;
                }
                let inst = &decoded[0];
                if inst.mnemonic.is_empty() || inst.mnemonic.to_lowercase() == "invalid" {
                    break;
                }

                boundaries.push((ip, ip + inst.length as u64));
                count += 1;

                match inst.flow_kind {
                    DecodedFlowKind::Return => {
                        break;
                    }
                    DecodedFlowKind::Jump => {
                        // Do not trace unconditional jumps to avoid enveloping tail call targets
                        break;
                    }
                    DecodedFlowKind::Call => {
                        let call_target = inst.direct_target.or_else(|| {
                            inst.references.iter().find_map(|r| match r.kind {
                                DecodedReferenceKind::CallTarget
                                | DecodedReferenceKind::RipRelativeAddress => Some(r.target),
                                _ => None,
                            })
                        });
                        let is_noreturn = if let Some(target) = call_target {
                            let norm_target = normalize_target(binary, target);
                            is_target_no_return(binary, norm_target)
                        } else {
                            false
                        };
                        if is_noreturn {
                            break;
                        }
                        ip += inst.length as u64;
                    }
                    DecodedFlowKind::ConditionalJump => {
                        let jump_target = inst.direct_target.or_else(|| {
                            inst.references.iter().find_map(|r| match r.kind {
                                DecodedReferenceKind::BranchTarget
                                | DecodedReferenceKind::RipRelativeAddress => Some(r.target),
                                _ => None,
                            })
                        });
                        if let Some(target) = jump_target {
                            let norm_target = normalize_target(binary, target);
                            let in_same_section =
                                binary.sections.is_empty() || same_section(binary, ip, norm_target);
                            if super::ranges::is_in_executable_ranges(norm_target, &exec_ranges)
                                && !known_functions.contains(&norm_target)
                                && !binary.function_addr_index.contains_key(&norm_target)
                                && in_same_section
                            {
                                worklist.push(norm_target);
                            }
                        }
                        ip += inst.length as u64;
                    }
                    DecodedFlowKind::Interrupt | DecodedFlowKind::Syscall => {
                        break;
                    }
                    _ => {
                        ip += inst.length as u64;
                    }
                }
            }
        }
    }
    boundaries
}

fn is_x86(binary: &LoadedBinary) -> bool {
    binary
        .architecture
        .as_ref()
        .is_some_and(|arch| arch.processor == "x86")
}

/// Decode the possible preceding instruction and check where it ends.
/// When a frontend is unavailable, use the architecture's alignment check.
fn non_x86_function_boundary(
    binary: &LoadedBinary,
    frontend: Option<&RuntimeSleighFrontend>,
    decode_context: Option<PackedContextOverride>,
    addr: u64,
) -> bool {
    let Some(alignment) = instruction_alignment(binary) else {
        return true;
    };
    if addr % alignment != 0 {
        return false;
    }
    let Some(frontend) = frontend else {
        return true;
    };
    // Widths an instruction of this architecture can have, smallest first.
    let widths: &[u64] = if alignment == 2 { &[2, 4] } else { &[4] };
    widths.iter().any(|width| {
        let Some(start) = addr.checked_sub(*width) else {
            return false;
        };
        let Some(bytes) = binary.view_bytes(start, *width as usize) else {
            return false;
        };
        let Ok(instruction) = frontend.decode_single_no_pcode(bytes, start, decode_context) else {
            return false;
        };
        instruction.length as u64 == *width
            && matches!(
                instruction.flow_kind,
                DecodedFlowKind::Return | DecodedFlowKind::Jump
            )
    })
}

/// Byte alignment every instruction of this architecture starts on, where it
/// has a fixed one. `None` for a variable-length architecture, whose
/// instructions may begin anywhere.
fn instruction_alignment(binary: &LoadedBinary) -> Option<u64> {
    let arch = binary.architecture.as_ref()?;
    match arch.processor.as_str() {
        // Thumb is 2-byte aligned and ARM mode 4; take the weaker of the two
        // rather than guess the mode, since a Thumb image's addresses do not
        // say which they are.
        "ARM" => Some(2),
        "AARCH64" | "MIPS" | "PowerPC" | "Sparc" | "RISCV" | "Loongarch" => Some(4),
        _ => None,
    }
}

/// Check for evidence of an instruction boundary before a possible function start.
/// The x86 path recognizes padding and terminating opcodes. Other processors
/// use the architecture's alignment and, when available, its SLEIGH decoder.
pub(super) fn is_plausible_function_boundary(
    binary: &LoadedBinary,
    frontend: Option<&RuntimeSleighFrontend>,
    addr: u64,
) -> bool {
    if !is_x86(binary) {
        let decode_context = frontend.and_then(|f| image_decode_context(binary, f));
        return non_x86_function_boundary(binary, frontend, decode_context, addr);
    }
    // Look back up to 16 bytes to find a boundary
    let mut check_addr = addr;
    let mut skipped = 0;
    while skipped < 16 && check_addr > 0 {
        check_addr -= 1;
        let Some(byte_slice) = binary.view_bytes(check_addr, 1) else {
            break;
        };
        let b = byte_slice[0];
        if b == 0xcc || b == 0x90 {
            skipped += 1;
            continue;
        }

        // We found a non-padding byte `b` at `check_addr`.
        // Check if the instruction ending at `check_addr` is a terminator.

        // 1. RET (0xc3)
        if b == 0xc3 {
            return true;
        }

        // 2. RET imm16 (0xc2 xx xx): last byte at check_addr, opcode at check_addr-2
        if check_addr >= 2 {
            if let Some(op) = binary.view_bytes(check_addr - 2, 1) {
                if op[0] == 0xc2 {
                    return true;
                }
            }
        }

        // 3. JMP short (0xeb xx): opcode at check_addr-1
        if check_addr >= 1 {
            if let Some(op) = binary.view_bytes(check_addr - 1, 1) {
                if op[0] == 0xeb {
                    return true;
                }
            }
        }

        // 4. JMP near (0xe9 xx xx xx xx): opcode at check_addr-4
        if check_addr >= 4 {
            if let Some(op) = binary.view_bytes_exact(check_addr - 4, 5) {
                if op[0] == 0xe9
                    && let Some(next_ip) = check_addr.checked_add(1)
                {
                    let rel32 = i32::from_le_bytes([op[1], op[2], op[3], op[4]]);
                    if let Some(target) = next_ip.checked_add_signed(i64::from(rel32)) {
                        let exec_ranges = super::ranges::executable_ranges(binary);
                        if super::ranges::is_in_executable_ranges(target, &exec_ranges) {
                            return true;
                        }
                    }
                }
            }
        }

        // 5. CALL near (0xe8 xx xx xx xx): opcode at check_addr-4
        //    CALL-terminated code is a valid boundary: the callee returns to
        //    the following byte, but the compiler may place a new subroutine there.
        if check_addr >= 4 {
            if let Some(op) = binary.view_bytes_exact(check_addr - 4, 5) {
                if op[0] == 0xe8
                    && let Some(next_ip) = check_addr.checked_add(1)
                {
                    let rel32 = i32::from_le_bytes([op[1], op[2], op[3], op[4]]);
                    if let Some(target) = next_ip.checked_add_signed(i64::from(rel32)) {
                        let exec_ranges = super::ranges::executable_ranges(binary);
                        if super::ranges::is_in_executable_ranges(target, &exec_ranges) {
                            return true;
                        }
                    }
                }
            }
        }

        // 6. JMP/CALL indirect via ModRM ff/2, ff/4, ff/d0..ff/d3 etc.
        //    These encode as: FF /4 (JMP [mem]) or FF /2 (CALL [mem])
        //    Min 2 bytes (FF D0), max 6 bytes (FF 15 addr32) before check_addr.
        // FF D0..FF D3 (CALL r/m32 with register operand), 2 bytes: opcode at check_addr-1
        if check_addr >= 1 {
            if let Some(op) = binary.view_bytes_exact(check_addr - 1, 2) {
                if op[0] == 0xff {
                    let modrm = op[1];
                    let reg = (modrm >> 3) & 0x7;
                    // /2 = CALL r/m, /4 = JMP r/m
                    if (reg == 2 || reg == 4) && (modrm >> 6) == 3 {
                        return true;
                    }
                }
            }
        }
        // FF /2 or FF /4 with ModRM+SIB (3-6 bytes): check a few offsets
        for back in [2usize, 5, 6] {
            if check_addr >= back as u64 {
                let op_addr = check_addr - back as u64;
                if let Some(op) = binary.view_bytes(op_addr, 1) {
                    if op[0] == 0xff
                        && let Some(mrm_addr) = op_addr.checked_add(1)
                        && let Some(mrm) = binary.view_bytes(mrm_addr, 1)
                    {
                        let reg = (mrm[0] >> 3) & 0x7;
                        if reg == 2 || reg == 4 {
                            return true;
                        }
                    }
                }
            }
        }

        break;
    }

    // If we skipped at least 4 bytes of padding, accept it even if we couldn't
    // resolve the preceding terminator opcode.
    if skipped >= 4 {
        return true;
    }

    false
}
