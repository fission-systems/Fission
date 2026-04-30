mod decode;
mod diagnostics;
mod engine;
mod frontend;
mod function;
mod lift;
pub mod native;
mod registry;
mod spine;

use std::collections::HashMap;
use std::fmt;

use anyhow::{anyhow, bail, Result};
use fission_core::architecture::BinaryLoadSpec;
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use serde::{Deserialize, Serialize};

use crate::compiler::{
    compile_frontend_for_entry_spec, discover_all_entry_specs, CompiledFrontend, EntrySpec,
};
pub use function::build_cfg_blocks;
pub use registry::{
    CompiledRuntimeRegistry, ExecutionEngineKey, ProcessorDescriptor, RuntimeEntrySelection,
    RuntimeEntrySelectionError, RuntimeEntrySelectionSource, RuntimeFrontendDescriptor,
    RuntimeSupportLevel, RuntimeVariantDescriptor,
};
pub use spine::{LanguageRuntime, ProcessorRuntimeProfile, RuntimeAttemptReport, RuntimeEndian};

const DEFAULT_FUNCTION_INSTRUCTION_LIMIT: usize = 512;

pub const UNIQUE_SPACE_ID: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeFrontendStatus {
    RegisteredCompileOnly,
    ExecutableCandidate,
}

impl RuntimeFrontendStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegisteredCompileOnly => "registered_compile_only",
            Self::ExecutableCandidate => "executable_candidate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSleighError {
    DecodeNoMatch {
        language: String,
        address: u64,
    },
    UnsupportedGeneratedSemantic {
        language: String,
        status: RuntimeFrontendStatus,
    },
    UnsupportedPcodeTemplate {
        language: String,
        reason: String,
    },
    InvalidPcodeShape {
        language: String,
        reason: String,
    },
}

impl fmt::Display for RuntimeSleighError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeNoMatch { language, address } => {
                write!(f, "DecodeNoMatch: {language} has no match at 0x{address:x}")
            }
            Self::UnsupportedGeneratedSemantic { language, status } => write!(
                f,
                "UnsupportedGeneratedSemantic: {language} runtime status is {}",
                status.as_str()
            ),
            Self::UnsupportedPcodeTemplate { language, reason } => {
                write!(f, "UnsupportedPcodeTemplate: {language}: {reason}")
            }
            Self::InvalidPcodeShape { language, reason } => {
                write!(f, "InvalidPcodeShape: {language}: {reason}")
            }
        }
    }
}

impl std::error::Error for RuntimeSleighError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionDetails {
    pub compat_emitter_used: bool,
    pub template_source: Option<crate::compiler::CompiledTemplateSource>,
}

use crate::runtime::native::NativeBackend;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RuntimeSleighFrontend {
    language: String,
    entry: EntrySpec,
    status: RuntimeFrontendStatus,
    compiled: Option<CompiledFrontend>,
    native_backend: Option<Arc<NativeBackend>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStopReason {
    TerminalControlFlow,
    InputExhausted,
    InstructionLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPcodeFunction {
    pub function: PcodeFunction,
    pub decoded_instructions: usize,
    pub stop_reason: DecodeStopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodedFlowKind {
    None,
    Jump,
    ConditionalJump,
    Call,
    Return,
    Interrupt,
    Syscall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodedReferenceKind {
    BranchTarget,
    CallTarget,
    MemoryAddress,
    ImmediateAddress,
    RipRelativeAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedReference {
    pub target: u64,
    pub kind: DecodedReferenceKind,
    pub operand_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedInstruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub length: usize,
    pub mnemonic: String,
    pub operands_text: String,
    pub flow_kind: DecodedFlowKind,
    pub direct_target: Option<u64>,
    pub references: Vec<DecodedReference>,
    /// Resolved ContextCommit entries from Ghidra's `globalset` directive.
    /// Each entry: (target_address, word_index, mask, value).
    /// Multi-instruction decoders should apply these to the context before
    /// decoding the instruction at `target_address`.
    #[serde(default)]
    pub pending_context_commits: Vec<(u64, u32, u32, u32)>,
}

impl DecodedInstruction {
    pub fn instruction_text(&self) -> String {
        if self.operands_text.is_empty() {
            self.mnemonic.clone()
        } else {
            format!("{} {}", self.mnemonic, self.operands_text)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeContract {
    pub instruction_limit: usize,
    pub stop_at_indirect_branch: bool,
}

impl DecodeContract {
    pub const fn strict_function(instruction_limit: usize) -> Self {
        Self {
            instruction_limit,
            stop_at_indirect_branch: true,
        }
    }

    pub const fn decomp_function(instruction_limit: usize) -> Self {
        Self {
            instruction_limit,
            stop_at_indirect_branch: false,
        }
    }

    pub const fn is_terminal_control_flow(self, opcode: PcodeOpcode) -> bool {
        matches!(opcode, PcodeOpcode::Return)
            || (self.stop_at_indirect_branch && matches!(opcode, PcodeOpcode::BranchInd))
    }
}

pub fn is_terminal_control_flow(opcode: PcodeOpcode) -> bool {
    DecodeContract::strict_function(DEFAULT_FUNCTION_INSTRUCTION_LIMIT)
        .is_terminal_control_flow(opcode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::GhidraLanguageManifest;
    use fission_core::architecture::BinaryLoadSpec;
    use fission_loader::loader::LoadedBinary;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    const GHIDRA_LANGUAGE_MANIFEST_TEST_JSON: &str =
        include_str!("../../specs/ghidra_language_manifest.json");

    fn var(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn runtime_registry_discovers_all_variants() {
        let manifest: GhidraLanguageManifest = serde_json::from_str(GHIDRA_LANGUAGE_MANIFEST_TEST_JSON)
            .expect("parse checked-in language manifest");
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        assert_eq!(registry.frontends().len(), manifest.variant_count);
        let x86_64 = registry.lookup("x86-64").expect("x86-64 registered");
        assert_eq!(x86_64.status, RuntimeFrontendStatus::ExecutableCandidate);
        assert_eq!(x86_64.processor, "x86");
        let aarch64 = registry
            .lookup("AARCH64:LE:64:v8A")
            .expect("AARCH64 language id registered");
        assert_eq!(aarch64.processor, "AARCH64");
        let arm_alias = registry
            .lookup("arm32")
            .expect("legacy ARM alias registered");
        assert_eq!(arm_alias.processor, "ARM");
    }

    #[test]
    fn runtime_registry_covers_all_ghidra_processors() {
        let manifest: GhidraLanguageManifest = serde_json::from_str(GHIDRA_LANGUAGE_MANIFEST_TEST_JSON)
            .expect("parse checked-in language manifest");
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        let manifest_processors = registry
            .frontends()
            .iter()
            .map(|frontend| frontend.processor.as_str())
            .collect::<BTreeSet<_>>();
        let registry_processors = registry
            .processors()
            .iter()
            .map(|descriptor| descriptor.ghidra_processor.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(manifest_processors.len(), manifest.processor_count);
        assert_eq!(registry_processors, manifest_processors);
        let expected_executable: Vec<&str> = manifest
            .entries
            .iter()
            .filter(|entry| entry.runtime_status == RuntimeFrontendStatus::ExecutableCandidate.as_str())
            .map(|entry| entry.processor.as_str())
            .collect();
        let actual_executable: Vec<&str> = registry
            .frontends()
            .iter()
            .filter(|frontend| frontend.status == RuntimeFrontendStatus::ExecutableCandidate)
            .map(|frontend| frontend.processor.as_str())
            .collect();
        assert_eq!(actual_executable, expected_executable);
    }

    #[test]
    fn riscv_now_resolves_as_executable_candidate() {
        let frontend = RuntimeSleighFrontend::new_for_language("RISCV:LE:64:default")
            .expect("RISCV runtime frontend");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    #[test]
    fn runtime_frontend_lifts_x86_64_ret_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("x86-64 runtime frontend");
        let decoded = frontend
            .decode_window(&[0xC3], 0x1000, 1)
            .expect("x86-64 ret decode");
        assert_eq!(
            decoded.first().map(|instruction| instruction.length),
            Some(1)
        );

        let (ops, length) = frontend
            .decode_and_lift_with_len(&[0xC3], 0x1000)
            .expect("ret should lift from .sla ConstructTpl");
        assert_eq!(length, 1);
        assert_eq!(
            ops.iter().map(|op| op.opcode).collect::<Vec<_>>(),
            vec![PcodeOpcode::Load, PcodeOpcode::IntAdd, PcodeOpcode::Return]
        );
    }

    #[test]
    fn runtime_registry_resolves_x86_64_load_spec_to_entry_id() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        let load_spec = BinaryLoadSpec::new(
            "PE",
            0x140000000,
            "x86:LE:64:default",
            "windows",
            "unit-test",
        );
        let selection = registry
            .resolve_from_load_spec(&load_spec)
            .expect("resolve x86-64 load spec");
        assert_eq!(selection.entry_id, "x86-64");
        assert_eq!(selection.processor, "x86");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    #[test]
    fn runtime_frontend_load_spec_matches_entry_id_frontend_for_ret() {
        let load_spec = BinaryLoadSpec::new(
            "PE",
            0x140000000,
            "x86:LE:64:default",
            "windows",
            "unit-test",
        );
        let from_load_spec =
            RuntimeSleighFrontend::new_for_load_spec(&load_spec).expect("load-spec runtime");
        let from_entry_id =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("entry-id runtime");

        let (load_spec_ops, load_spec_len) = from_load_spec
            .decode_and_lift_with_len(&[0xC3], 0x1000)
            .expect("load-spec ret lift");
        let (entry_ops, entry_len) = from_entry_id
            .decode_and_lift_with_len(&[0xC3], 0x1000)
            .expect("entry-id ret lift");

        assert_eq!(from_load_spec.language(), "x86-64");
        assert_eq!(load_spec_len, entry_len);
        assert_eq!(load_spec_ops, entry_ops);
    }

    #[test]
    fn runtime_frontend_load_spec_matches_entry_id_on_failing_test_functions_row() {
        let binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark/binary/x86-64/window/small/binary/c/test_functions.exe");
        let binary = LoadedBinary::from_file(&binary_path).expect("load test_functions.exe");
        let entry_address = 0x140001450_u64;
        let bytes = binary
            .view_bytes(entry_address, 16)
            .expect("view bytes for failing row");
        let load_spec = binary.load_spec().expect("binary load spec").clone();

        let from_load_spec =
            RuntimeSleighFrontend::new_for_load_spec(&load_spec).expect("load-spec runtime");
        let from_entry_id =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("entry-id runtime");

        let load_spec_result = from_load_spec.decode_and_lift_with_len(bytes, entry_address);
        let entry_id_result = from_entry_id.decode_and_lift_with_len(bytes, entry_address);

        assert_eq!(
            load_spec_result.is_ok(),
            entry_id_result.is_ok(),
            "load-spec and entry-id frontends diverged on test_functions:add @ 0x140001450"
        );
        match (load_spec_result, entry_id_result) {
            (Ok((lhs_ops, lhs_len)), Ok((rhs_ops, rhs_len))) => {
                assert_eq!(lhs_len, rhs_len);
                assert_eq!(lhs_ops, rhs_ops);
            }
            (Err(lhs_err), Err(rhs_err)) => {
                assert_eq!(format!("{lhs_err:#}"), format!("{rhs_err:#}"));
            }
            _ => unreachable!("success mismatch handled above"),
        }
    }

    #[test]
    fn cfg_blocks_conditional_branch_has_target_and_fallthrough() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x10, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(2, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x110, 8), Varnode::constant(1, 1)],
            ),
            op(
                2,
                0x108,
                PcodeOpcode::IntAdd,
                Some(var(0x20, 4)),
                vec![Varnode::constant(3, 4), Varnode::constant(4, 4)],
            ),
            op(3, 0x10c, PcodeOpcode::Return, None, vec![]),
            op(
                4,
                0x110,
                PcodeOpcode::IntAdd,
                Some(var(0x30, 4)),
                vec![Varnode::constant(5, 4), Varnode::constant(6, 4)],
            ),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[1].start_address, 0x108);
        assert_eq!(blocks[2].start_address, 0x110);
        assert_eq!(blocks[0].successors, vec![2, 1]);
    }
}
