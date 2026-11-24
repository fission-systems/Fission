//! Executable-section xref scanning shared by the flow projection and full index.

use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReferenceKind, PackedContextOverride,
    RuntimeSleighFrontend,
};

use super::{OPERAND_INDEX_MNEMONIC, Xref, XrefType};
use crate::analysis::xref_coverage::XrefOmissionReason;

/// Project calls and jumps from the same executable-section decode as the xref index.
pub(in crate::analysis) fn visit_code_flow_xrefs(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    mut visit: impl FnMut(Xref),
) {
    scan_executable_code(binary, frontend, |instruction| {
        visit_instruction_xrefs(&instruction, None, &mut visit);
    });
}

pub(super) fn scan_executable_code(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    mut visit: impl FnMut(DecodedInstruction),
) -> (usize, Vec<XrefOmissionReason>) {
    let mut completed = 0;
    let mut omitted = Vec::new();
    for section in binary.executable_sections() {
        if section.file_size == 0 {
            continue;
        }
        let Some(code) = usize::try_from(section.file_offset)
            .ok()
            .and_then(|start| {
                usize::try_from(section.file_size)
                    .ok()
                    .and_then(|sz| start.checked_add(sz))
                    .map(|end| (start, end))
            })
            .and_then(|(start, end)| binary.data.as_slice().get(start..end))
        else {
            omitted.push(XrefOmissionReason::SectionBytesUnavailable);
            continue;
        };
        let address_state = frontend.normalize_low_bit_code_address(section.virtual_address);
        let base_addr = address_state.address;
        let context_override = crate::analysis::decode_context_for_address(
            binary,
            frontend,
            section.virtual_address,
            address_state.context_override,
        );
        if let Some(reason) = decode_code(frontend, code, base_addr, context_override, &mut visit) {
            omitted.push(reason);
        } else {
            completed += 1;
        }
    }
    (completed, omitted)
}

fn decode_code(
    frontend: &RuntimeSleighFrontend,
    code: &[u8],
    base_addr: u64,
    context_override: Option<PackedContextOverride>,
    visit: &mut impl FnMut(DecodedInstruction),
) -> Option<XrefOmissionReason> {
    let decode_result = if context_override.is_some() {
        frontend.decode_window_with_context_override(code, base_addr, usize::MAX, context_override)
    } else {
        frontend.decode_window_parallel(code, base_addr)
    };
    let Ok(instructions) = decode_result else {
        return Some(XrefOmissionReason::InstructionDecodeFailed);
    };
    let decoded_bytes = instructions
        .iter()
        .map(|instruction| usize::from(instruction.length))
        .sum::<usize>();
    let omission = if decoded_bytes < code.len() {
        Some(if instructions.is_empty() {
            XrefOmissionReason::InstructionDecodeFailed
        } else {
            XrefOmissionReason::UndecodedExecutableTail
        })
    } else {
        None
    };

    instructions.into_iter().for_each(visit);
    omission
}

pub(super) fn visit_instruction_xrefs(
    instruction: &DecodedInstruction,
    mapped: Option<&[(u64, u64)]>,
    mut visit: impl FnMut(Xref),
) {
    let mut emitted_direct_target = false;
    for reference in &instruction.references {
        let xref_type = xref_type_from_sleigh_kind(reference.kind);
        let is_flow = matches!(xref_type, XrefType::Call | XrefType::Jump);
        // Keep data references into any mapped section, including .rodata and .data.
        if !is_flow
            && !mapped.is_some_and(|ranges| {
                ranges
                    .iter()
                    .any(|(start, end)| (*start..*end).contains(&reference.target))
            })
        {
            continue;
        }
        emitted_direct_target |= is_flow && Some(reference.target) == instruction.direct_target;
        visit(Xref {
            from_addr: instruction.address,
            to_addr: reference.target,
            xref_type,
            operand_index: usize_to_operand_index(reference.operand_index),
            sleigh_kind: Some(reference.kind),
            flow_kind: is_flow.then_some(instruction.flow_kind),
        });
    }

    if let Some(target) = instruction.direct_target
        && !emitted_direct_target
    {
        let xref_type = match instruction.flow_kind {
            DecodedFlowKind::Call => XrefType::Call,
            DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => XrefType::Jump,
            _ => return,
        };
        visit(Xref {
            from_addr: instruction.address,
            to_addr: target,
            xref_type,
            operand_index: OPERAND_INDEX_MNEMONIC,
            sleigh_kind: None,
            flow_kind: Some(instruction.flow_kind),
        });
    }
}

#[inline]
fn xref_type_from_sleigh_kind(kind: DecodedReferenceKind) -> XrefType {
    match kind {
        DecodedReferenceKind::CallTarget => XrefType::Call,
        DecodedReferenceKind::BranchTarget => XrefType::Jump,
        DecodedReferenceKind::MemoryAddress
        | DecodedReferenceKind::ImmediateAddress
        | DecodedReferenceKind::RipRelativeAddress => XrefType::Data,
    }
}

#[inline]
fn usize_to_operand_index(op: usize) -> i32 {
    i32::try_from(op).unwrap_or(i32::MAX)
}
