use anyhow::Result;

use crate::lifter::SleighLifter;

use super::ir::CompiledFrontend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionSample {
    pub source: String,
    pub address: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EquivalenceMismatchKind {
    DecodeLengthMismatch,
    ControlFlowMismatch,
    PcodeOpcodeMismatch,
    VarnodeShapeMismatch,
    UnsupportedGeneratedSemantic,
}

impl EquivalenceMismatchKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DecodeLengthMismatch => "decode_length_mismatch",
            Self::ControlFlowMismatch => "control_flow_mismatch",
            Self::PcodeOpcodeMismatch => "pcode_opcode_mismatch",
            Self::VarnodeShapeMismatch => "varnode_shape_mismatch",
            Self::UnsupportedGeneratedSemantic => "unsupported_generated_semantic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivalenceRecord {
    pub source: String,
    pub address: u64,
    pub bytes_hex: String,
    pub hand_decode_len: Option<u64>,
    pub hand_control_flow: String,
    pub hand_pcode_opcodes: Vec<String>,
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
    let lifter = SleighLifter::new_for_language("x86-64")?;
    let mut totals = std::collections::BTreeMap::<String, usize>::new();
    let mut records = Vec::new();

    for sample in samples {
        let (hand_decode_len, hand_control_flow, hand_pcode_opcodes) =
            match lifter.decode_and_lift_with_len(&sample.bytes, sample.address) {
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
                    (Some(len), control_flow, opcode_names)
                }
                Err(err) => (
                    None,
                    format!("decode_error:{err:#}"),
                    Vec::new(),
                ),
            };

        // Compiler-only wave: the generated frontend has no executable decoder yet.
        // Keep the report explicit rather than pretending to compare runtime semantics.
        let mismatch = EquivalenceMismatchKind::UnsupportedGeneratedSemantic;
        *totals.entry(mismatch.as_str().to_string()).or_insert(0) += 1;
        let _ = compiled;
        records.push(EquivalenceRecord {
            source: sample.source.clone(),
            address: sample.address,
            bytes_hex: hex_bytes(&sample.bytes),
            hand_decode_len,
            hand_control_flow,
            hand_pcode_opcodes,
            mismatch,
        });
    }

    Ok(EquivalenceReport {
        sample_count: records.len(),
        mismatch_totals: totals.into_iter().collect(),
        records,
    })
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
    bytes.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fission_loader::LoadedBinary;

    use super::*;
    use crate::compiler::compile_x86_64_frontend;

    #[test]
    fn equivalence_report_runs_for_unit_seeds() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let report = build_x86_64_equivalence_report(&compiled, &default_unit_seed_samples())
            .expect("equivalence report");
        assert_eq!(report.sample_count, 5);
        assert!(report
            .mismatch_totals
            .iter()
            .any(|(kind, count)| kind == "unsupported_generated_semantic" && *count == 5));
    }

    #[test]
    fn equivalence_report_accepts_benchmark_binary_samples() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
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

    fn benchmark_function_entry_samples(path: &Path, limit: usize) -> Result<Vec<InstructionSample>> {
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
