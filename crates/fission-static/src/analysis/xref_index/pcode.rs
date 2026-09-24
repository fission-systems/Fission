//! P-code-derived cross-reference extraction with bounded value-set analysis.

use fission_loader::loader::LoadedBinary;
use fission_pcode::PcodeOpcode;
use fission_sleigh::runtime::{DecodeStopReason, DecodedFlowKind, RuntimeSleighFrontend};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::analysis::xref_coverage::{
    XrefAnalysisLayer, XrefAnalysisState, XrefCoverageUnit, XrefLayerCoverage, XrefOmissionReason,
    XrefUnsupportedReason,
};

use super::build::XrefIndexBuilder;
use super::model::{
    XrefEvidence, XrefKind, XrefSource, XrefSourceCategory, XrefSourceLayer, XrefTarget,
};
use crate::analysis::value_set::{ValueSetAnalyzer, VsaFact};

const PCODE_FUNCTION_INSTRUCTION_LIMIT: usize = 4096;
const PCODE_FUNCTION_BYTE_LIMIT: u64 = 1 << 20;

/// Add references proven by bounded p-code value-set analysis.
///
/// Only complete, file-backed function lifts are analyzed. Data targets must
/// lie in a mapped section; computed flow targets must lie in executable
/// sections or be known import slots. Unsupported/unknown values produce no
/// record rather than promoting arbitrary immediates to addresses.
pub fn push_pcode_layer(builder: &mut XrefIndexBuilder, binary: &LoadedBinary) {
    let _ = push_pcode_layer_with_coverage(builder, binary);
}

pub(crate) fn push_pcode_layer_with_coverage(
    builder: &mut XrefIndexBuilder,
    binary: &LoadedBinary,
) -> XrefLayerCoverage {
    let mut functions: Vec<_> = binary
        .functions
        .iter()
        .filter(|function| !function.is_import)
        .collect();
    functions.sort_by_key(|function| function.address);

    let mut coverage = XrefLayerCoverage::requested(
        XrefAnalysisLayer::Pcode,
        "discovered non-import functions with a complete file-backed extent, at most 1 MiB and 4,096 decoded instructions, terminal control flow, and successful value-set analysis",
        XrefCoverageUnit::DiscoveredFunction,
    );
    coverage.candidate_units = functions.len();
    let initial_pcode_records = builder.pending_layer_count(XrefSourceLayer::Pcode);

    let Some(load_spec) = binary.load_spec() else {
        coverage.mark_unsupported(XrefUnsupportedReason::LoadSpecUnavailable);
        coverage.finalize();
        return coverage;
    };
    let frontend = match RuntimeSleighFrontend::new_for_load_spec(load_spec) {
        Ok(frontend) => frontend,
        Err(_) => {
            coverage.mark_unsupported(XrefUnsupportedReason::SleighFrontendUnavailable);
            coverage.finalize();
            return coverage;
        }
    };
    let Some(ram_space) = frontend.compiled_frontend().and_then(|compiled| {
        compiled
            .sla_spaces
            .values()
            .find(|space| space.name.eq_ignore_ascii_case("ram"))
    }) else {
        coverage.mark_unsupported(XrefUnsupportedReason::RamSpaceUnavailable);
        coverage.finalize();
        return coverage;
    };
    let ram_space_id = ram_space.index;
    let ram_addressable_unit_bytes = u64::from(ram_space.word_size);
    if ram_addressable_unit_bytes == 0 {
        coverage.mark_unsupported(XrefUnsupportedReason::InvalidRamAddressableUnit);
        coverage.finalize();
        return coverage;
    }

    let mut emitted = FxHashSet::default();

    for function in functions {
        if function.size == 0 {
            coverage.omit(XrefOmissionReason::MissingFunctionExtent, 1);
            continue;
        }
        if function.size > PCODE_FUNCTION_BYTE_LIMIT {
            coverage.omit(XrefOmissionReason::FunctionOverByteLimit, 1);
            continue;
        }
        let Ok(size) = usize::try_from(function.size) else {
            coverage.omit(XrefOmissionReason::FunctionSizeUnrepresentable, 1);
            continue;
        };
        let Some(available) = binary.available_execution_bytes(function.address) else {
            coverage.omit(XrefOmissionReason::FunctionBytesUnavailable, 1);
            continue;
        };
        if available < size {
            coverage.omit(XrefOmissionReason::FunctionBytesUnavailable, 1);
            continue;
        }
        let Some(bytes) = binary.view_executable_bytes(function.address, size) else {
            coverage.omit(XrefOmissionReason::FunctionBytesUnavailable, 1);
            continue;
        };
        let Ok(decoded) = frontend.lift_raw_pcode_function_with_contract(
            bytes,
            function.address,
            PCODE_FUNCTION_INSTRUCTION_LIMIT,
        ) else {
            coverage.omit(XrefOmissionReason::FunctionLiftFailed, 1);
            continue;
        };
        if decoded.stop_reason != DecodeStopReason::TerminalControlFlow {
            coverage.omit(
                match decoded.stop_reason {
                    DecodeStopReason::InputExhausted => {
                        XrefOmissionReason::FunctionLiftInputExhausted
                    }
                    DecodeStopReason::InstructionLimit => {
                        XrefOmissionReason::FunctionLiftInstructionLimit
                    }
                    DecodeStopReason::TerminalControlFlow => {
                        XrefOmissionReason::FunctionLiftNotTerminal
                    }
                },
                1,
            );
            continue;
        }
        let decoded_instructions: FxHashMap<_, _> = decoded
            .instructions
            .iter()
            .map(|instruction| {
                (
                    instruction.address,
                    (instruction.flow_kind, instruction.length as u64),
                )
            })
            .collect();

        let mut analyzer = ValueSetAnalyzer::new();
        if !analyzer.analyze(&decoded.function) {
            coverage.omit(XrefOmissionReason::ValueSetAnalysisIncomplete, 1);
            continue;
        }
        coverage.completed_units += 1;

        for fact in &analyzer.facts {
            let (source, target, kind, pcode_op) = match fact {
                VsaFact::DataRead {
                    instruction_addr,
                    target_addr,
                    pcode_op: PcodeOpcode::Load,
                } => (
                    *instruction_addr,
                    *target_addr,
                    XrefKind::DataRead,
                    PcodeOpcode::Load,
                ),
                VsaFact::DataWrite {
                    instruction_addr,
                    target_addr,
                    pcode_op: PcodeOpcode::Store,
                } => (
                    *instruction_addr,
                    *target_addr,
                    XrefKind::DataWrite,
                    PcodeOpcode::Store,
                ),
                VsaFact::JumpTableTarget {
                    instruction_addr,
                    targets,
                    pcode_op,
                } => {
                    let Some((decoded_flow_kind, instruction_length)) =
                        decoded_instructions.get(instruction_addr)
                    else {
                        continue;
                    };
                    let Some(kind) = pcode_flow_xref_kind(*pcode_op, *decoded_flow_kind) else {
                        continue;
                    };
                    for target in targets {
                        if is_fallthrough_target(*instruction_addr, *instruction_length, *target) {
                            continue;
                        }
                        if !is_executable_target(binary, *target)
                            && !binary.iat_symbols.contains_key(target)
                        {
                            continue;
                        }
                        if !emitted.insert((*instruction_addr, *target, kind, *pcode_op)) {
                            continue;
                        }
                        builder.push_record(
                            XrefSource {
                                address: *instruction_addr,
                                category: XrefSourceCategory::Instruction {
                                    enclosing_function: Some(function.address),
                                },
                            },
                            XrefTarget {
                                address: Some(*target),
                                symbol: pcode_target_symbol(binary, *target),
                            },
                            kind,
                            fission_loader::Confidence::Medium,
                            XrefEvidence {
                                layer: XrefSourceLayer::Pcode,
                                instruction_mnemonic: None,
                                pcode_op: Some(format!("{pcode_op:?}").to_ascii_uppercase()),
                                relocation_kind: None,
                                relocation_type: None,
                                relocation_size: None,
                                relocation_addend: None,
                                symbol_name: pcode_target_symbol(binary, *target),
                                note: Some(
                                    "bounded value-set resolved indirect flow target".into(),
                                ),
                            },
                        );
                    }
                    continue;
                }
                _ => continue,
            };

            if !is_mapped_target(binary, target)
                || !emitted.insert((source, target, kind, pcode_op))
            {
                continue;
            }
            builder.push_record(
                XrefSource {
                    address: source,
                    category: XrefSourceCategory::Instruction {
                        enclosing_function: Some(function.address),
                    },
                },
                XrefTarget {
                    address: Some(target),
                    symbol: pcode_target_symbol(binary, target),
                },
                kind,
                fission_loader::Confidence::Medium,
                XrefEvidence {
                    layer: XrefSourceLayer::Pcode,
                    instruction_mnemonic: None,
                    pcode_op: Some(format!("{pcode_op:?}").to_ascii_uppercase()),
                    relocation_kind: None,
                    relocation_type: None,
                    relocation_size: None,
                    relocation_addend: None,
                    symbol_name: pcode_target_symbol(binary, target),
                    note: Some("bounded value-set resolved memory address".into()),
                },
            );
        }

        // Sleigh may encode an absolute memory operand as a RAM-space varnode
        // directly (for example, x86 RIP-relative `COPY`) rather than as a
        // `LOAD`/`STORE` pair. The address-space identity comes from the
        // compiled language metadata; registers and unique temporaries are
        // intentionally ignored here.
        for op in decoded.function.blocks.iter().flat_map(|block| &block.ops) {
            if matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::Branch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::CBranch
            ) {
                let Some((decoded_flow_kind, instruction_length)) =
                    decoded_instructions.get(&op.address)
                else {
                    // P-code branches can implement instruction semantics (for
                    // example CMOV) without representing machine-level flow.
                    continue;
                };
                let Some(kind) = pcode_flow_xref_kind(op.opcode, *decoded_flow_kind) else {
                    continue;
                };
                if let Some(target) = op.inputs.first().and_then(|varnode| {
                    direct_ram_target(varnode, ram_space_id, ram_addressable_unit_bytes)
                }) {
                    if is_fallthrough_target(op.address, *instruction_length, target) {
                        continue;
                    }
                    push_pcode_reference(
                        builder,
                        &mut emitted,
                        binary,
                        function.address,
                        op.address,
                        target,
                        kind,
                        op.opcode,
                        "p-code RAM-space operand resolves a flow target",
                    );
                }
                continue;
            }

            for input in &op.inputs {
                if let Some(target) =
                    direct_ram_target(input, ram_space_id, ram_addressable_unit_bytes)
                {
                    push_pcode_reference(
                        builder,
                        &mut emitted,
                        binary,
                        function.address,
                        op.address,
                        target,
                        XrefKind::DataRead,
                        op.opcode,
                        "p-code input reads a direct RAM-space varnode",
                    );
                }
            }
            if let Some(target) = op.output.as_ref().and_then(|output| {
                direct_ram_target(output, ram_space_id, ram_addressable_unit_bytes)
            }) {
                push_pcode_reference(
                    builder,
                    &mut emitted,
                    binary,
                    function.address,
                    op.address,
                    target,
                    XrefKind::DataWrite,
                    op.opcode,
                    "p-code output writes a direct RAM-space varnode",
                );
            }
        }
    }

    coverage.records_emitted = builder
        .pending_layer_count(XrefSourceLayer::Pcode)
        .saturating_sub(initial_pcode_records);
    coverage.finalize();
    coverage
}

fn pcode_flow_xref_kind(
    opcode: PcodeOpcode,
    decoded_flow_kind: DecodedFlowKind,
) -> Option<XrefKind> {
    match (opcode, decoded_flow_kind) {
        (PcodeOpcode::Call | PcodeOpcode::CallInd, DecodedFlowKind::Call) => Some(XrefKind::Call),
        (PcodeOpcode::Branch | PcodeOpcode::BranchInd, DecodedFlowKind::Jump) => {
            Some(XrefKind::Jump)
        }
        (PcodeOpcode::CBranch, DecodedFlowKind::ConditionalJump) => Some(XrefKind::ConditionalJump),
        _ => None,
    }
}

fn is_fallthrough_target(instruction_address: u64, instruction_length: u64, target: u64) -> bool {
    instruction_address.checked_add(instruction_length) == Some(target)
}

fn direct_ram_target(
    varnode: &fission_pcode::Varnode,
    ram_space_id: u64,
    addressable_unit_bytes: u64,
) -> Option<u64> {
    (!varnode.is_constant && varnode.space_id == ram_space_id)
        .then(|| varnode.offset.checked_mul(addressable_unit_bytes))
        .flatten()
}

fn push_pcode_reference(
    builder: &mut XrefIndexBuilder,
    emitted: &mut FxHashSet<(u64, u64, XrefKind, PcodeOpcode)>,
    binary: &LoadedBinary,
    function_address: u64,
    source_address: u64,
    target_address: u64,
    kind: XrefKind,
    pcode_op: PcodeOpcode,
    note: &str,
) {
    let valid_target = match kind {
        XrefKind::Call | XrefKind::Jump | XrefKind::ConditionalJump => {
            is_executable_target(binary, target_address)
                || binary.iat_symbols.contains_key(&target_address)
        }
        XrefKind::DataRead | XrefKind::DataWrite => is_mapped_target(binary, target_address),
        _ => false,
    };
    if !valid_target || !emitted.insert((source_address, target_address, kind, pcode_op)) {
        return;
    }
    let symbol = pcode_target_symbol(binary, target_address);
    builder.push_record(
        XrefSource {
            address: source_address,
            category: XrefSourceCategory::Instruction {
                enclosing_function: Some(function_address),
            },
        },
        XrefTarget {
            address: Some(target_address),
            symbol: symbol.clone(),
        },
        kind,
        fission_loader::Confidence::Medium,
        XrefEvidence {
            layer: XrefSourceLayer::Pcode,
            instruction_mnemonic: None,
            pcode_op: Some(format!("{pcode_op:?}").to_ascii_uppercase()),
            relocation_kind: None,
            relocation_type: None,
            relocation_size: None,
            relocation_addend: None,
            symbol_name: symbol,
            note: Some(note.to_string()),
        },
    );
}

fn is_mapped_target(binary: &LoadedBinary, target: u64) -> bool {
    binary.sections.iter().any(|section| {
        let end = section
            .virtual_address
            .saturating_add(section.virtual_size.max(section.file_size));
        target >= section.virtual_address && target < end
    })
}

fn is_executable_target(binary: &LoadedBinary, target: u64) -> bool {
    binary.executable_section_containing(target).is_some()
}

fn pcode_target_symbol(binary: &LoadedBinary, target: u64) -> Option<String> {
    binary
        .iat_symbols
        .get(&target)
        .or_else(|| binary.global_symbols.get(&target))
        .cloned()
        .or_else(|| {
            binary
                .function_at_exact(target)
                .map(|function| function.name.clone())
        })
}
