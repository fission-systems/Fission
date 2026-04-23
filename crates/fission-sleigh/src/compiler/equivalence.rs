use anyhow::Result;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeParityFixture {
    pub processor: String,
    pub entry_id: String,
    pub source: String,
    pub address: u64,
    pub bytes_hex: String,
    pub expected_decode_len: Option<u64>,
    pub expected_constructor_path: Vec<String>,
    pub expected_pcode_opcodes: Vec<String>,
    pub expected_varnode_shapes: Vec<RuntimeParityVarnodeShape>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeParityVarnodeShape {
    pub space_id: u64,
    pub size: u32,
    pub is_constant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeParityReport {
    pub processor: String,
    pub entry_id: String,
    pub fixture_count: usize,
    pub catastrophic_mismatch_count: usize,
    pub mismatch_totals: Vec<(String, usize)>,
    pub records: Vec<RuntimeParityRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeParityRecord {
    pub source: String,
    pub address: u64,
    pub bytes_hex: String,
    pub expected_decode_len: Option<u64>,
    pub actual_decode_len: Option<u64>,
    pub mismatch: String,
    pub actual_pcode_opcodes: Vec<String>,
}

pub fn build_runtime_fixture_report(
    processor: &str,
    entry_id: &str,
    fixtures: &[RuntimeParityFixture],
    mut decode: impl FnMut(&RuntimeParityFixture) -> Result<(Vec<fission_pcode::PcodeOp>, u64)>,
) -> RuntimeParityReport {
    let mut totals = std::collections::BTreeMap::<String, usize>::new();
    let mut catastrophic_mismatch_count = 0usize;
    let mut records = Vec::new();

    for fixture in fixtures {
        let actual = decode(fixture);
        let (actual_decode_len, actual_pcode_opcodes, actual_ops) = match actual.as_ref() {
            Ok((ops, len)) => (
                Some(*len),
                ops.iter()
                    .map(|op| format!("{:?}", op.opcode))
                    .collect::<Vec<_>>(),
                ops.clone(),
            ),
            Err(_) => (None, Vec::new(), Vec::new()),
        };
        let mismatch = classify_fixture_mismatch(fixture, actual_decode_len, &actual_ops);
        if matches!(
            mismatch,
            EquivalenceMismatchKind::DecisionTreeNoMatch
                | EquivalenceMismatchKind::ConstructorSelectionMismatch
        ) {
            catastrophic_mismatch_count = catastrophic_mismatch_count.saturating_add(1);
        }
        *totals.entry(mismatch.as_str().to_string()).or_insert(0) += 1;
        records.push(RuntimeParityRecord {
            source: fixture.source.clone(),
            address: fixture.address,
            bytes_hex: fixture.bytes_hex.clone(),
            expected_decode_len: fixture.expected_decode_len,
            actual_decode_len,
            mismatch: mismatch.as_str().to_string(),
            actual_pcode_opcodes,
        });
    }

    RuntimeParityReport {
        processor: processor.to_string(),
        entry_id: entry_id.to_string(),
        fixture_count: fixtures.len(),
        catastrophic_mismatch_count,
        mismatch_totals: totals.into_iter().collect(),
        records,
    }
}

fn classify_fixture_mismatch(
    fixture: &RuntimeParityFixture,
    actual_decode_len: Option<u64>,
    actual_ops: &[fission_pcode::PcodeOp],
) -> EquivalenceMismatchKind {
    let Some(actual_len) = actual_decode_len else {
        return EquivalenceMismatchKind::DecisionTreeNoMatch;
    };
    if fixture
        .expected_decode_len
        .is_some_and(|expected| expected != actual_len)
    {
        return EquivalenceMismatchKind::DecodeLengthMismatch;
    }
    let actual_opcodes = actual_ops
        .iter()
        .map(|op| format!("{:?}", op.opcode))
        .collect::<Vec<_>>();
    if !fixture.expected_pcode_opcodes.is_empty()
        && fixture.expected_pcode_opcodes != actual_opcodes
    {
        return EquivalenceMismatchKind::PcodeOpcodeMismatch;
    }
    let actual_shapes = actual_ops
        .iter()
        .flat_map(|op| op.output.iter().chain(op.inputs.iter()))
        .map(|vn| RuntimeParityVarnodeShape {
            space_id: vn.space_id,
            size: vn.size,
            is_constant: vn.is_constant,
        })
        .collect::<Vec<_>>();
    if !fixture.expected_varnode_shapes.is_empty()
        && fixture.expected_varnode_shapes != actual_shapes
    {
        return EquivalenceMismatchKind::VarnodeShapeMismatch;
    }
    EquivalenceMismatchKind::ExactParity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_fixture_report_buckets_catastrophic_mismatches() {
        let fixtures = vec![RuntimeParityFixture {
            processor: "x86".to_string(),
            entry_id: "x86-64".to_string(),
            source: "unit-fixture:ret".to_string(),
            address: 0x1000,
            bytes_hex: "c3".to_string(),
            expected_decode_len: Some(1),
            expected_constructor_path: Vec::new(),
            expected_pcode_opcodes: vec!["Return".to_string()],
            expected_varnode_shapes: Vec::new(),
        }];
        let report = build_runtime_fixture_report("x86", "x86-64", &fixtures, |_fixture| {
            Err(anyhow::anyhow!("forced fixture decode miss"))
        });

        assert_eq!(report.fixture_count, 1);
        assert_eq!(report.catastrophic_mismatch_count, 1);
        assert_eq!(
            report.mismatch_totals,
            vec![("decision_tree_no_match".to_string(), 1)]
        );
    }
}
