use anyhow::Result;

use crate::runtime::{decode_and_lift_x86_64_bridge, decode_and_lift_x86_64_generated};

use super::ir::CompiledFrontend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionSample {
    pub source: String,
    pub address: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EquivalenceMismatchKind {
    ExactParity,
    DecodeLengthMismatch,
    DecisionTreeNoMatch,
    ConstructorSelectionMismatch,
    OperandBindingMismatch,
    SemanticTemplateUnsupported,
    ControlFlowMismatch,
    PcodeOpcodeMismatch,
    VarnodeShapeMismatch,
    BranchTargetMismatch,
    TemporarySpaceMismatch,
}

impl EquivalenceMismatchKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExactParity => "exact_parity",
            Self::DecodeLengthMismatch => "decode_length_mismatch",
            Self::DecisionTreeNoMatch => "decision_tree_no_match",
            Self::ConstructorSelectionMismatch => "constructor_selection_mismatch",
            Self::OperandBindingMismatch => "operand_binding_mismatch",
            Self::SemanticTemplateUnsupported => "semantic_template_unsupported",
            Self::ControlFlowMismatch => "control_flow_mismatch",
            Self::PcodeOpcodeMismatch => "pcode_opcode_mismatch",
            Self::VarnodeShapeMismatch => "varnode_shape_mismatch",
            Self::BranchTargetMismatch => "branch_target_mismatch",
            Self::TemporarySpaceMismatch => "temporary_space_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivalenceRecord {
    pub source: String,
    pub address: u64,
    pub bytes_hex: String,
    pub hand_decode_len: Option<u64>,
    pub generated_decode_len: Option<u64>,
    pub hand_control_flow: String,
    pub generated_control_flow: String,
    pub hand_pcode_opcodes: Vec<String>,
    pub generated_pcode_opcodes: Vec<String>,
    pub mismatch: EquivalenceMismatchKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivalenceReport {
    pub sample_count: usize,
    pub mismatch_totals: Vec<(String, usize)>,
    pub records: Vec<EquivalenceRecord>,
}

pub fn build_x86_64_equivalence_report(
    compiled: &CompiledFrontend,
    samples: &[InstructionSample],
) -> Result<EquivalenceReport> {
    let mut totals = std::collections::BTreeMap::<String, usize>::new();
    let mut records = Vec::new();

    for sample in samples {
        let hand = decode_and_lift_x86_64_bridge(&sample.bytes, sample.address);
        let generated = decode_and_lift_x86_64_generated(compiled, &sample.bytes, sample.address);
        let (hand_decode_len, hand_control_flow, hand_pcode_opcodes, hand_ops) =
            decode_result_summary(hand.as_ref());
        let (generated_decode_len, generated_control_flow, generated_pcode_opcodes, generated_ops) =
            decode_result_summary(generated.as_ref());

        let mismatch = classify_mismatch(
            hand.as_ref().ok(),
            generated.as_ref().ok(),
            &hand_ops,
            &generated_ops,
        );
        *totals.entry(mismatch.as_str().to_string()).or_insert(0) += 1;
        records.push(EquivalenceRecord {
            source: sample.source.clone(),
            address: sample.address,
            bytes_hex: hex_bytes(&sample.bytes),
            hand_decode_len,
            generated_decode_len,
            hand_control_flow,
            generated_control_flow,
            hand_pcode_opcodes,
            generated_pcode_opcodes,
            mismatch,
        });
    }

    Ok(EquivalenceReport {
        sample_count: records.len(),
        mismatch_totals: totals.into_iter().collect(),
        records,
    })
}

fn decode_result_summary(
    result: Result<&(Vec<fission_pcode::PcodeOp>, u64), &anyhow::Error>,
) -> (
    Option<u64>,
    String,
    Vec<String>,
    Vec<fission_pcode::PcodeOp>,
) {
    match result {
        Ok((ops, len)) => {
            let control_flow = ops
                .iter()
                .find_map(|op| match op.opcode {
                    fission_pcode::PcodeOpcode::Branch => Some("branch"),
                    fission_pcode::PcodeOpcode::CBranch => Some("conditional_branch"),
                    fission_pcode::PcodeOpcode::BranchInd => Some("indirect_branch"),
                    fission_pcode::PcodeOpcode::Call => Some("call"),
                    fission_pcode::PcodeOpcode::CallInd => Some("indirect_call"),
                    fission_pcode::PcodeOpcode::Return => Some("return"),
                    _ => None,
                })
                .unwrap_or("linear")
                .to_string();
            let opcode_names = ops
                .iter()
                .map(|op| format!("{:?}", op.opcode))
                .collect::<Vec<_>>();
            (Some(*len), control_flow, opcode_names, ops.clone())
        }
        Err(err) => (None, format!("decode_error:{err:#}"), Vec::new(), Vec::new()),
    }
}

fn classify_mismatch(
    hand: Option<&(Vec<fission_pcode::PcodeOp>, u64)>,
    generated: Option<&(Vec<fission_pcode::PcodeOp>, u64)>,
    hand_ops: &[fission_pcode::PcodeOp],
    generated_ops: &[fission_pcode::PcodeOp],
) -> EquivalenceMismatchKind {
    match (hand, generated) {
        (Some(_), None) => EquivalenceMismatchKind::DecisionTreeNoMatch,
        (None, Some(_)) => EquivalenceMismatchKind::ConstructorSelectionMismatch,
        (None, None) => EquivalenceMismatchKind::DecisionTreeNoMatch,
        (Some((_, hand_len)), Some((_, generated_len))) => {
            if hand_len != generated_len {
                return EquivalenceMismatchKind::DecodeLengthMismatch;
            }
            let hand_branch = branch_targets(hand_ops);
            let generated_branch = branch_targets(generated_ops);
            if hand_branch != generated_branch {
                return EquivalenceMismatchKind::BranchTargetMismatch;
            }
            let hand_temps = temp_shapes(hand_ops);
            let generated_temps = temp_shapes(generated_ops);
            if hand_temps != generated_temps {
                return EquivalenceMismatchKind::TemporarySpaceMismatch;
            }
            let hand_opcodes = hand_ops
                .iter()
                .map(|op| format!("{:?}", op.opcode))
                .collect::<Vec<_>>();
            let generated_opcodes = generated_ops
                .iter()
                .map(|op| format!("{:?}", op.opcode))
                .collect::<Vec<_>>();
            if hand_opcodes != generated_opcodes {
                return EquivalenceMismatchKind::PcodeOpcodeMismatch;
            }
            if hand_ops
                .iter()
                .zip(generated_ops.iter())
                .any(|(lhs, rhs)| lhs.inputs.len() != rhs.inputs.len() || lhs.output.as_ref().map(varnode_shape) != rhs.output.as_ref().map(varnode_shape))
            {
                return EquivalenceMismatchKind::VarnodeShapeMismatch;
            }
            if hand_ops
                .iter()
                .zip(generated_ops.iter())
                .any(|(lhs, rhs)| lhs.inputs.iter().map(varnode_shape).collect::<Vec<_>>() != rhs.inputs.iter().map(varnode_shape).collect::<Vec<_>>())
            {
                return EquivalenceMismatchKind::OperandBindingMismatch;
            }
            EquivalenceMismatchKind::ExactParity
        }
    }
}

fn branch_targets(ops: &[fission_pcode::PcodeOp]) -> Vec<u64> {
    ops.iter()
        .filter_map(|op| match op.opcode {
            fission_pcode::PcodeOpcode::Branch | fission_pcode::PcodeOpcode::CBranch => op
                .inputs
                .first()
                .filter(|vn| vn.is_constant)
                .map(|vn| vn.constant_val as u64),
            _ => None,
        })
        .collect()
}

fn temp_shapes(ops: &[fission_pcode::PcodeOp]) -> Vec<(u64, u32)> {
    ops.iter()
        .flat_map(|op| op.output.iter().chain(op.inputs.iter()))
        .filter(|vn| !vn.is_constant && vn.space_id == crate::runtime::UNIQUE_SPACE_ID)
        .map(|vn| (vn.offset, vn.size))
        .collect()
}

fn varnode_shape(vn: &fission_pcode::Varnode) -> (u64, u32, bool) {
    (vn.space_id, vn.size, vn.is_constant)
}

pub fn default_unit_seed_samples() -> Vec<InstructionSample> {
    vec![
        InstructionSample {
            source: "unit-seed:nop".to_string(),
            address: 0x1000,
            bytes: vec![0x90],
        },
        InstructionSample {
            source: "unit-seed:return".to_string(),
            address: 0x1010,
            bytes: vec![0xC3],
        },
        InstructionSample {
            source: "unit-seed:jump".to_string(),
            address: 0x1020,
            bytes: vec![0xEB, 0x02, 0x90],
        },
        InstructionSample {
            source: "unit-seed:call".to_string(),
            address: 0x1030,
            bytes: vec![0xE8, 0x05, 0x00, 0x00, 0x00],
        },
        InstructionSample {
            source: "unit-seed:cmp-jne".to_string(),
            address: 0x1040,
            bytes: vec![0x39, 0xD8, 0x75, 0x01, 0x90],
        },
    ]
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fission_loader::LoadedBinary;

    use super::*;
    use crate::compiler::{compile_frontend_for_entry_spec, x86_64_entry_spec_path};

    #[test]
    fn equivalence_report_runs_for_unit_seeds() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        let report = build_x86_64_equivalence_report(&compiled, &default_unit_seed_samples())
            .expect("equivalence report");
        assert_eq!(report.sample_count, 5);
        assert!(!report.mismatch_totals.is_empty());
        assert_eq!(report.records.len(), 5);
    }

    #[test]
    fn equivalence_report_accepts_benchmark_binary_samples() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        let samples = benchmark_function_entry_samples(
            Path::new(
                "/Users/sjkim1127/Fission/benchmark/binary/x86-64/window/small/binary/c/test_functions.exe",
            ),
            4,
        )
        .expect("benchmark samples");
        assert!(!samples.is_empty());
        let report =
            build_x86_64_equivalence_report(&compiled, &samples).expect("equivalence report");
        assert_eq!(report.sample_count, samples.len());
    }

    fn benchmark_function_entry_samples(
        path: &Path,
        limit: usize,
    ) -> Result<Vec<InstructionSample>> {
        let binary = LoadedBinary::from_file(path)?;
        let mut samples = Vec::new();
        for function in binary
            .functions_sorted()
            .into_iter()
            .filter(|function| !function.is_import && function.address != 0)
            .take(limit)
        {
            let Some(bytes) = binary.get_bytes(function.address, 16) else {
                continue;
            };
            samples.push(InstructionSample {
                source: format!(
                    "{}:{}@0x{:x}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("<binary>"),
                    function.name,
                    function.address
                ),
                address: function.address,
                bytes,
            });
        }
        Ok(samples)
    }
}
