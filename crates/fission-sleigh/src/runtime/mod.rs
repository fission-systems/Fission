mod address_state;
mod decode;
mod diagnostics;
mod engine;
mod frontend;
mod function;
mod lift;
mod registry;
mod spine;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use anyhow::{anyhow, bail, Result};
use fission_core::architecture::BinaryLoadSpec;
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use serde::{Deserialize, Serialize};

use crate::compiler::{
    compile_frontend_for_entry_spec, discover_all_entry_specs, CompiledFrontend, EntrySpec,
};
pub use crate::packed_context::PackedContextOverride;
pub use address_state::RuntimeAddressState;
pub use function::{
    build_cfg_blocks, build_cfg_blocks_from_ops, build_cfg_blocks_with_hints,
    build_instruction_cfg_snapshot,
};
pub use registry::{
    CompiledRuntimeRegistry, ExecutionEngineKey, ProcessorDescriptor, RuntimeEntrySelection,
    RuntimeEntrySelectionError, RuntimeEntrySelectionSource, RuntimeFrontendDescriptor,
    RuntimeSupportLevel, RuntimeVariantDescriptor,
};
pub use spine::{LanguageRuntime, ProcessorRuntimeProfile, RuntimeAttemptReport, RuntimeEndian};

/// Extract the register name → (offset, size) map for the given `BinaryLoadSpec`.
///
/// Reads the checked-in packaged `.sla` file (Ghidra's canonical `ELEM_VARNODE_SYM` table)
/// from `utils/sleigh-specs/compiled/` and converts it into the flat register map.
///
/// Returns `None` if the SLA file is unavailable or the library cannot be decoded.
pub fn register_map_for_load_spec(
    load_spec: &BinaryLoadSpec,
) -> Option<std::collections::HashMap<String, (u64, u32)>> {
    use crate::compiler::{packaged_sla_for_entry_spec, sla::load_construct_templates_from_sla};

    let language_id = load_spec.pair.language_id.as_str();

    let entries = discover_all_entry_specs().ok()?;
    let entry = entries
        .into_iter()
        .find(|e| frontend::entry_matches_language_name(e, language_id))?;

    let sla_path = packaged_sla_for_entry_spec(&entry.path).ok()??;
    let library = load_construct_templates_from_sla(&sla_path).ok()?;

    let map = library
        .register_map
        .into_iter()
        .map(|(name, varnode)| (name, (varnode.offset, varnode.size)))
        .collect();

    Some(map)
}

const DEFAULT_FUNCTION_INSTRUCTION_LIMIT: usize = 512;

pub const UNIQUE_SPACE_ID: u64 = 3;

fn checked_instruction_fallthrough(address: u64, length: u64) -> Result<u64> {
    address
        .checked_add(length)
        .ok_or_else(|| anyhow!("instruction address overflow at 0x{address:x} length {length}"))
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionDetails {
    pub template_source: Option<crate::compiler::CompiledTemplateSource>,
    /// Resolved ContextCommit entries emitted while binding this instruction.
    /// Each entry: (target_address, word_index, mask, value).
    #[serde(default)]
    pub pending_context_commits: Vec<(u64, u32, u32, u32)>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSleighFrontend {
    language: String,
    entry: EntrySpec,
    status: RuntimeFrontendStatus,
    compiled: Option<CompiledFrontend>,
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
    pub template_source_counts: BTreeMap<String, usize>,
    /// Reachable instruction start addresses in ascending order (includes zero-pcode nops).
    pub reachable_instruction_addresses: Vec<u64>,
    /// Decoded byte length per reachable instruction address.
    pub instruction_lengths: BTreeMap<u64, u64>,
    /// Indirect branch targets inferred during lift, keyed by branch site address.
    pub inferred_indirect_edges: BTreeMap<u64, Vec<u64>>,
    /// Union of inferred and jump-table indirect branch targets.
    pub indirect_targets: BTreeSet<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodeMemoryContext {
    pub relative_address_bases: Vec<u64>,
    pub jump_table_targets: Vec<u64>,
    /// Loader-provided label addresses (for example COFF `.l_startw`) within the lifted function.
    pub block_entry_hints: Vec<u64>,
    /// Flow-reference targets (jump/jcc destinations) promoted to block leaders.
    pub flow_leaders: Vec<u64>,
    /// Explicit non-call flow edges `(from_instruction, to)` within the lifted function.
    pub flow_edges: Vec<(u64, u64)>,
    /// Call instruction addresses whose callee is known to never return.
    pub noreturn_callsites: Vec<u64>,
}

/// Merged instruction-level CFG hints consumed by `build_instruction_cfg_snapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstructionCfgHints {
    pub block_leaders: BTreeSet<u64>,
    pub flow_edges: Vec<(u64, u64)>,
    pub noreturn_callsites: BTreeSet<u64>,
}

impl InstructionCfgHints {
    pub fn from_memory_context(ctx: &DecodeMemoryContext) -> Self {
        let mut block_leaders = BTreeSet::new();
        block_leaders.extend(ctx.block_entry_hints.iter().copied());
        block_leaders.extend(ctx.flow_leaders.iter().copied());
        Self {
            block_leaders,
            flow_edges: ctx.flow_edges.clone(),
            noreturn_callsites: ctx.noreturn_callsites.iter().copied().collect(),
        }
    }
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
    use crate::compiler::discovery;
    use fission_core::architecture::BinaryLoadSpec;
    use std::collections::BTreeSet;

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
    fn runtime_instruction_address_advance_fails_on_overflow() {
        assert_eq!(checked_instruction_fallthrough(0x1000, 4).unwrap(), 0x1004);
        assert!(checked_instruction_fallthrough(u64::MAX, 1).is_err());
    }

    #[test]
    fn runtime_decode_and_lift_do_not_saturate_instruction_addresses() {
        let decode_source = include_str!("decode.rs");
        let lift_source = include_str!("lift.rs");
        let current_saturating = ["current", "saturating_add"].join(".");
        let seq_saturating = ["global_seq", "saturating_add"].join(".");

        assert!(
            !decode_source.contains(&current_saturating),
            "decode traversal must fail on instruction address overflow"
        );
        assert!(
            !lift_source.contains(&current_saturating) && !lift_source.contains(&seq_saturating),
            "function lifting must fail on instruction address or seq_num overflow"
        );
    }

    #[test]
    fn cfg_blocks_split_symbolic_internal_targets_without_size_gate() {
        let mut ops = Vec::new();
        for idx in 0..48u32 {
            let address = 0x1000 + u64::from(idx);
            let op = if idx == 4 {
                op(
                    idx,
                    address,
                    PcodeOpcode::CBranch,
                    None,
                    vec![var(0x1020, 8), var(0x7000, 1)],
                )
            } else {
                op(
                    idx,
                    address,
                    PcodeOpcode::Copy,
                    Some(var(0x8000 + u64::from(idx), 8)),
                    vec![var(0x9000 + u64::from(idx), 8)],
                )
            };
            ops.push(op);
        }

        let blocks = build_cfg_blocks_from_ops(0x1000, ops, &BTreeSet::new());
        assert!(
            blocks.iter().any(|block| block.start_address == 0x1020),
            "{blocks:?}"
        );
        assert!(
            blocks.iter().any(|block| block
                .ops
                .last()
                .is_some_and(|op| op.address == 0x1004 && block.successors.len() == 2)),
            "{blocks:?}"
        );
    }

    #[test]
    fn runtime_registry_discovers_all_variants() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        assert_eq!(registry.frontends().len(), 146);
        let x86_64 = registry.lookup("x86-64").expect("x86-64 registered");
        assert_eq!(x86_64.status, RuntimeFrontendStatus::ExecutableCandidate);
        assert_eq!(x86_64.processor, "x86");
        let aarch64 = registry
            .lookup("AARCH64:LE:64:v8A")
            .expect("AARCH64 language id registered");
        assert_eq!(aarch64.processor, "AARCH64");
        let arm_alias = registry
            .lookup("arm32")
            .expect("ARM language alias registered");
        assert_eq!(arm_alias.processor, "ARM");
    }

    #[test]
    fn runtime_registry_covers_all_ghidra_processors() {
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

        assert_eq!(manifest_processors.len(), 38);
        assert_eq!(registry_processors, manifest_processors);
        // All 146 variants are promoted to ExecutableCandidate.
        assert!(registry
            .frontends()
            .iter()
            .all(|frontend| frontend.status == RuntimeFrontendStatus::ExecutableCandidate));
    }

    #[test]
    fn loongarch_variants_lift_add_w_from_spec_template() {
        for language in [
            "Loongarch:LE:32:ilp32f",
            "Loongarch:LE:32:ilp32d",
            "Loongarch:LE:64:lp64f",
            "Loongarch:LE:64:lp64d",
        ] {
            let frontend =
                RuntimeSleighFrontend::new_for_language(language).expect("LoongArch runtime");
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );

            let bytes = [0xa4, 0x10, 0x10, 0x00]; // add.w a0, a1, a0
            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .expect("LoongArch add.w decode");
            assert_eq!(
                decoded.first().map(|instruction| instruction.length),
                Some(4),
                "{language}"
            );

            let (ops, length) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .expect("LoongArch add.w should lift from .sla ConstructTpl");
            assert_eq!(length, 4, "{language}");
            assert!(!ops.is_empty(), "{language}");
        }
    }

    #[test]
    fn sparc_v9_64_lifts_add_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("sparc:BE:64:default")
            .expect("SPARC V9 64 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        let bytes = [0x90, 0x02, 0x40, 0x08]; // add %o1, %o0, %o0
        let decoded = frontend
            .decode_window(&bytes, 0x1000, 1)
            .expect("SPARC add decode");
        assert_eq!(
            decoded.first().map(|instruction| instruction.length),
            Some(4)
        );

        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x1000)
            .expect("SPARC add should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(!ops.is_empty());
    }

    #[test]
    fn powerpc_32_defaults_lift_add_from_spec_template() {
        for (language, bytes) in [
            ("PowerPC:BE:32:default", [0x7c, 0x64, 0x1a, 0x14]),
            ("PowerPC:LE:32:default", [0x14, 0x1a, 0x64, 0x7c]),
        ] {
            let frontend =
                RuntimeSleighFrontend::new_for_language(language).expect("PPC32 runtime");
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );

            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .expect("PPC32 add decode");
            assert_eq!(
                decoded.first().map(|instruction| instruction.length),
                Some(4),
                "{language}"
            );

            let (ops, length) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .expect("PPC32 add should lift from .sla ConstructTpl");
            assert_eq!(length, 4, "{language}");
            assert!(!ops.is_empty(), "{language}");
        }
    }

    #[test]
    fn powerpc_64_defaults_lift_add_from_spec_template() {
        for (language, bytes) in [
            (
                "PowerPC:BE:64:default",
                [0x7c, 0x64, 0x1a, 0x14, 0x78, 0x63, 0x00, 0x20],
            ),
            (
                "PowerPC:LE:64:default",
                [0x14, 0x1a, 0x64, 0x7c, 0x20, 0x00, 0x63, 0x78],
            ),
        ] {
            let frontend =
                RuntimeSleighFrontend::new_for_language(language).expect("PPC64 runtime");
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );

            let decoded = frontend
                .decode_window(&bytes, 0x1000, 2)
                .expect("PPC64 add decode");
            assert_eq!(
                decoded.first().map(|instruction| instruction.length),
                Some(4),
                "{language}"
            );

            let (ops, length) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .expect("PPC64 add should lift from .sla ConstructTpl");
            assert_eq!(length, 4, "{language}");
            assert!(!ops.is_empty(), "{language}");
        }
    }

    #[test]
    fn powerpc_64_powerisa_lifts_iselgt_from_spec_template() {
        for (language, bytes) in [
            ("PowerPC:BE:64:A2ALT", [0x7c, 0x66, 0x28, 0x5e]),
            ("PowerPC:LE:64:A2ALT", [0x5e, 0x28, 0x66, 0x7c]),
        ] {
            let frontend =
                RuntimeSleighFrontend::new_for_language(language).expect("PPC64 PowerISA runtime");
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );

            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .expect("PPC64 iselgt decode");
            assert_eq!(
                decoded.first().map(|instruction| instruction.length),
                Some(4),
                "{language}"
            );
            assert_eq!(
                decoded
                    .first()
                    .map(|instruction| instruction.mnemonic.as_str()),
                Some("iselgt"),
                "{language}"
            );

            let (ops, length) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .expect("PPC64 iselgt should lift from .sla ConstructTpl");
            assert_eq!(length, 4, "{language}");
            assert!(!ops.is_empty(), "{language}");
        }
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
    fn runtime_function_lift_follows_conditional_target_after_fallthrough_ret() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("x86-64 runtime frontend");
        let bytes = [
            0x85, 0xd2, // test EDX,EDX
            0x7e, 0x03, // jle 0x1007
            0x31, 0xc0, // xor EAX,EAX
            0xc3, // ret
            0xb8, 0x01, 0x00, 0x00, 0x00, // mov EAX,1
            0xc3, // ret
        ];

        let lifted = frontend
            .lift_raw_pcode_function_with_decode_contract(
                &bytes,
                0x1000,
                DecodeContract::strict_function(16),
            )
            .expect("branch target after fallthrough ret should lift");

        assert_eq!(lifted.decoded_instructions, 6);
        assert!(
            lifted
                .function
                .blocks
                .iter()
                .any(|block| block.start_address == 0x1007),
            "{:?}",
            lifted.function.blocks
        );
        assert_eq!(
            lifted
                .function
                .blocks
                .iter()
                .flat_map(|block| &block.ops)
                .filter(|op| op.opcode == PcodeOpcode::Return)
                .count(),
            2
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
    fn arm_low_bit_code_address_seeds_thumb_context_without_address_byte_skew() {
        let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").expect("ARM8 runtime");
        let address_state = frontend.normalize_low_bit_code_address(0x100001);
        let decode_address = address_state.address;
        let context_override = address_state.context_override;
        assert_eq!(decode_address, 0x100000);
        assert!(context_override.is_some());

        let bytes = [0x4c, 0xf6, 0xcd, 0x41];
        let (ops, length, details) = frontend
            .decode_and_lift_with_context_override(&bytes, decode_address, context_override)
            .expect("Thumb low-bit code pointer should decode from aligned bytes");
        assert_eq!(length, 4);
        assert_eq!(
            details.template_source,
            Some(crate::compiler::CompiledTemplateSource::SpecDerived)
        );
        assert!(ops.iter().any(|op| {
            op.opcode == PcodeOpcode::IntZExt
                && op
                    .inputs
                    .first()
                    .is_some_and(|input| input.is_constant && input.constant_val == 0xcccd)
        }));
    }

    #[test]
    fn arm_low_bit_decode_window_keeps_thumb_context_after_first_instruction() {
        let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").expect("ARM8 runtime");
        let address_state = frontend.normalize_low_bit_code_address(0x100001);
        assert_eq!(address_state.address, 0x100000);
        assert!(address_state.context_override.is_some());

        let bytes = [0x4c, 0xf6, 0xcd, 0x41, 0xcc, 0xf6, 0xcc, 0x41];
        let decoded = frontend
            .decode_window_with_context_override(
                &bytes,
                address_state.address,
                2,
                address_state.context_override,
            )
            .expect("Thumb decode window should keep low-bit context across instructions");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].address, 0x100000);
        assert_eq!(decoded[0].length, 4);
        assert_eq!(decoded[0].mnemonic, "movw");
        assert_eq!(decoded[1].address, 0x100004);
        assert_eq!(decoded[1].length, 4);
        assert_eq!(decoded[1].mnemonic, "movt");
    }

    #[test]
    fn arm8m_recursive_thumb_subtables_decode_without_sequential_recursion_failure() {
        for (entry_id, bytes) in [("ARM8m_le", [0xb0, 0xb5]), ("ARM8m_be", [0xb5, 0xb0])] {
            let frontend =
                RuntimeSleighFrontend::new_for_language(entry_id).expect("ARM8m runtime");
            let address_state = frontend.normalize_low_bit_code_address(0x100001);
            assert_eq!(address_state.address, 0x100000);
            let decoded = frontend
                .decode_instruction_with_context_override(
                    &bytes,
                    address_state.address,
                    address_state.context_override,
                )
                .unwrap_or_else(|err| panic!("{entry_id} Thumb push decode: {err:#}"));
            assert_eq!(decoded.length, 2, "{entry_id} Thumb push length");
            assert_eq!(decoded.mnemonic, "push", "{entry_id} Thumb push mnemonic");

            let (ops, length, details) = frontend
                .decode_and_lift_with_context_override(
                    &bytes,
                    address_state.address,
                    address_state.context_override,
                )
                .unwrap_or_else(|err| panic!("{entry_id} Thumb push lift: {err:#}"));
            assert_eq!(length, 2, "{entry_id} Thumb push lift length");
            assert_eq!(
                details.template_source,
                Some(crate::compiler::CompiledTemplateSource::SpecDerived),
                "{entry_id} Thumb push should stay on SLA ConstructTpl"
            );
            assert!(!ops.is_empty(), "{entry_id} Thumb push should emit p-code");
        }
    }

    #[test]
    fn arm8m_be_low_bit_code_address_decodes_thumb_instruction_without_byte_skew() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("ARM8m_be").expect("ARM8m_be runtime");
        let address_state = frontend.normalize_low_bit_code_address(0x100019);
        assert_eq!(address_state.address, 0x100018);
        assert!(address_state.context_override.is_some());

        let bytes = [0xeb, 0x00, 0x00, 0x80];
        let decoded = frontend
            .decode_instruction_with_context_override(
                &bytes,
                address_state.address,
                address_state.context_override,
            )
            .expect("ARM8m_be Thumb low-bit code pointer should decode from aligned bytes");
        assert_eq!(decoded.address, 0x100018);
        assert_eq!(decoded.length, 4);
        assert_eq!(decoded.mnemonic, "add.w");

        let (ops, length, details) = frontend
            .decode_and_lift_with_context_override(
                &bytes,
                address_state.address,
                address_state.context_override,
            )
            .expect("ARM8m_be Thumb low-bit code pointer should lift from aligned bytes");
        assert_eq!(length, 4);
        assert_eq!(
            details.template_source,
            Some(crate::compiler::CompiledTemplateSource::SpecDerived)
        );
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    }

    #[test]
    fn arm_thumb_it_context_commit_keeps_conditional_bx_fallthrough_reachable() {
        for (language, bytes) in [
            (
                "ARM8_le",
                [
                    0x4c, 0xf6, 0xcd, 0x41, // movw r1,#0xcccd
                    0xcc, 0xf6, 0xcc, 0x41, // movt r1,#0xcccc
                    0xa0, 0xfb, 0x01, 0x12, // umull r1,r2,r0,r1
                    0x91, 0x08, // lsrs r1,r2,#0x2
                    0x01, 0xeb, 0x81, 0x01, // add.w r1,r1,r1, lsl #0x2
                    0x41, 0x1a, // subs r1,r0,r1
                    0x03, 0x29, // cmp r1,#0x3
                    0x88, 0xbf, // it hi
                    0x70, 0x47, // bx lr
                    0xdf, 0xe8, 0x01, 0xf0, // tbb [pc,r1]
                ],
            ),
            (
                "ARM8_be",
                [
                    0xf6, 0x4c, 0x41, 0xcd, // movw r1,#0xcccd
                    0xf6, 0xcc, 0x41, 0xcc, // movt r1,#0xcccc
                    0xfb, 0xa0, 0x12, 0x01, // umull r1,r2,r0,r1
                    0x08, 0x91, // lsrs r1,r2,#0x2
                    0xeb, 0x01, 0x01, 0x81, // add.w r1,r1,r1, lsl #0x2
                    0x1a, 0x41, // subs r1,r0,r1
                    0x29, 0x03, // cmp r1,#0x3
                    0xbf, 0x88, // it hi
                    0x47, 0x70, // bx lr
                    0xe8, 0xdf, 0xf0, 0x01, // tbb [pc,r1]
                ],
            ),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language).expect("ARM runtime");
            let address_state = frontend.normalize_low_bit_code_address(0x100001);
            let lifted = frontend
                .lift_raw_pcode_function_with_context_and_memory_context(
                    &bytes,
                    address_state.address,
                    DecodeContract::strict_function(32),
                    &DecodeMemoryContext::default(),
                    address_state.context_override,
                )
                .unwrap_or_else(|err| panic!("{language} Thumb IT function lift: {err:#}"));

            assert!(
                lifted
                    .function
                    .blocks
                    .iter()
                    .any(|block| block.start_address == 0x10001a),
                "{language} TBB fallthrough after conditional bx must remain reachable: {:?}",
                lifted.function.blocks
            );
        }
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

        let blocks = build_cfg_blocks_from_ops(0x100, ops, &BTreeSet::new());
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[1].start_address, 0x108);
        assert_eq!(blocks[2].start_address, 0x110);
        assert_eq!(blocks[0].successors, vec![1, 2]);
    }

    #[test]
    fn cfg_blocks_keeps_instruction_local_relative_conditional_in_one_block() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::Int2Comp,
                Some(var(0x10, 4)),
                vec![var(0x20, 4)],
            ),
            op(
                1,
                0x100,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(2, 8), var(0x30, 1)],
            ),
            op(
                2,
                0x100,
                PcodeOpcode::Copy,
                Some(var(0x10, 4)),
                vec![var(0x20, 4)],
            ),
            op(
                3,
                0x100,
                PcodeOpcode::IntZExt,
                Some(var(0x40, 8)),
                vec![var(0x10, 4)],
            ),
            op(4, 0x104, PcodeOpcode::Return, None, vec![]),
        ];

        let blocks = build_cfg_blocks_from_ops(0x100, ops, &BTreeSet::new());
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[0].ops.len(), 5);
        assert_eq!(blocks[0].ops.last().unwrap().opcode, PcodeOpcode::Return);
        assert!(blocks[0].successors.is_empty());
    }

    #[test]
    fn cfg_blocks_split_instruction_local_relative_unconditional_target() {
        let ops = vec![
            op(
                0,
                0x200,
                PcodeOpcode::Copy,
                Some(var(0x10, 4)),
                vec![Varnode::constant(1, 4)],
            ),
            op(
                1,
                0x200,
                PcodeOpcode::Branch,
                None,
                vec![Varnode::constant(2, 8)],
            ),
            op(
                2,
                0x200,
                PcodeOpcode::Copy,
                Some(var(0x10, 4)),
                vec![Varnode::constant(2, 4)],
            ),
            op(
                3,
                0x200,
                PcodeOpcode::Copy,
                Some(var(0x20, 4)),
                vec![var(0x10, 4)],
            ),
        ];

        let blocks = build_cfg_blocks_from_ops(0x200, ops, &BTreeSet::new());
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].start_address, 0x200);
        assert_eq!(blocks[0].ops.len(), 4);
        assert!(blocks[0].successors.is_empty());
    }

    #[test]
    fn mips32_le_lifts_addu_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("MIPS:LE:32:default")
            .expect("MIPS32 LE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // addu v0, a0, a1  (LE encoding: 21 10 85 00)
        let bytes = [0x21, 0x10, 0x85, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS32 LE addu decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(4),
            "MIPS32 LE addu length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS32 LE addu should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(
            !ops.is_empty(),
            "MIPS32 LE addu emitted no p-code; ops={ops:?}"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected MIPS addu to emit INT_ADD; ops={ops:?}"
        );
    }

    #[test]
    fn mips32_be_lifts_addu_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("MIPS:BE:32:default")
            .expect("MIPS32 BE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // addu v0, a0, a1  (BE encoding: 00 85 10 21)
        let bytes = [0x00, 0x85, 0x10, 0x21];
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS32 BE addu decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(4),
            "MIPS32 BE addu length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS32 BE addu should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(
            !ops.is_empty(),
            "MIPS32 BE addu emitted no p-code; ops={ops:?}"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected MIPS addu to emit INT_ADD; ops={ops:?}"
        );
    }

    #[test]
    fn mips64_be_lifts_addiu_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("MIPS:BE:64:default")
            .expect("MIPS64 BE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // addiu v0, zero, 42  (BE encoding: 24 02 00 2a)
        let bytes = [0x24, 0x02, 0x00, 0x2a];
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS64 BE addiu decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(4),
            "MIPS64 BE addiu length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS64 BE addiu should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(
            !ops.is_empty(),
            "MIPS64 BE addiu emitted no p-code; ops={ops:?}"
        );
    }

    #[test]
    fn p6502_lifts_lda_immediate_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("6502:LE:16:default").expect("6502 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // LDA #42  (0xA9 0x2A)
        let bytes = [0xa9, 0x2a];
        let decoded = frontend
            .decode_window(&bytes, 0x8000, 1)
            .expect("6502 LDA decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(2),
            "6502 LDA immediate length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x8000)
            .expect("6502 LDA should lift from .sla ConstructTpl");
        assert_eq!(length, 2);
        assert!(!ops.is_empty(), "6502 LDA emitted no p-code; ops={ops:?}");
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::Copy),
            "expected 6502 LDA to emit COPY; ops={ops:?}"
        );
    }

    #[test]
    fn avr8_lifts_add_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("avr8:LE:16:default").expect("AVR8 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // AVR8 NOP = 0x0000 (LE: 00 00)
        let bytes = [0x00, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x0000, 1)
            .expect("AVR8 NOP decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(2),
            "AVR8 NOP length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x0000)
            .expect("AVR8 NOP should lift from .sla ConstructTpl");
        assert_eq!(length, 2);
        // NOP may emit zero ops or a no-op COPY; both are acceptable
        let _ = ops;
    }

    #[test]
    fn m68000_lifts_nop_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("68000:BE:32:default").expect("68000 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // 68040 NOP = 0x4E71 (BE: 4e 71)
        let bytes = [0x4e, 0x71];
        let decoded = frontend
            .decode_window(&bytes, 0x1000, 1)
            .expect("68000 NOP decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(2),
            "68000 NOP length"
        );
        let (_ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x1000)
            .expect("68000 NOP should lift from .sla ConstructTpl");
        assert_eq!(length, 2);
    }

    #[test]
    fn riscv_32_le_lifts_addi_from_spec_template() {
        // Note: RISCV:LE:32 uses riscv.ilp32d.slaspec entry
        let frontend = RuntimeSleighFrontend::new_for_language("RISCV:LE:32:default")
            .expect("RISC-V 32 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // addi x10, x10, 1  => 0x00150513
        // LE bytes: 13 05 15 00
        let bytes = [0x13, 0x05, 0x15, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x10000, 1)
            .expect("RISC-V 32 addi decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(4),
            "RISC-V 32 addi length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x10000)
            .expect("RISC-V 32 addi should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(
            !ops.is_empty(),
            "RISC-V 32 addi emitted no p-code; ops={ops:?}"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected RISC-V addi to emit INT_ADD; ops={ops:?}"
        );
    }

    #[test]
    fn riscv_64_le_lifts_addi_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("RISCV:LE:64:default")
            .expect("RISC-V 64 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );

        // addi x10, x10, 1 => LE bytes: 13 05 15 00
        let bytes = [0x13, 0x05, 0x15, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x10000, 1)
            .expect("RISC-V 64 addi decode");
        assert_eq!(
            decoded.first().map(|i| i.length),
            Some(4),
            "RISC-V 64 addi length"
        );
        let (ops, length) = frontend
            .decode_and_lift_with_len(&bytes, 0x10000)
            .expect("RISC-V 64 addi should lift from .sla ConstructTpl");
        assert_eq!(length, 4);
        assert!(
            !ops.is_empty(),
            "RISC-V 64 addi emitted no p-code; ops={ops:?}"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected RISC-V addi to emit INT_ADD; ops={ops:?}"
        );
    }

    // ── MIPS R6 / MIPS64 LE ──────────────────────────────────────────────
    #[test]
    fn mips32_r6_be_lifts_addiu_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("MIPS:BE:32:R6").expect("MIPS32 R6 BE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0x24, 0x02, 0x00, 0x2a]; // addiu v0, zero, 42
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS32 R6 BE decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(4));
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS32 R6 BE lift");
        assert_eq!(len, 4);
        assert!(
            !ops.is_empty(),
            "MIPS32 R6 BE emitted no p-code; ops={ops:?}"
        );
    }

    #[test]
    fn mips32_r6_le_lifts_addiu_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("MIPS:LE:32:R6").expect("MIPS32 R6 LE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0x2a, 0x00, 0x02, 0x24]; // addiu v0, zero, 42 LE
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS32 R6 LE decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(4));
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS32 R6 LE lift");
        assert_eq!(len, 4);
        assert!(
            !ops.is_empty(),
            "MIPS32 R6 LE emitted no p-code; ops={ops:?}"
        );
    }

    #[test]
    fn mips64_le_lifts_addu_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("MIPS:LE:64:default")
            .expect("MIPS64 LE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0x21, 0x10, 0x85, 0x00]; // addu v0, a0, a1 LE
        let decoded = frontend
            .decode_window(&bytes, 0x400000, 1)
            .expect("MIPS64 LE decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(4));
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x400000)
            .expect("MIPS64 LE lift");
        assert_eq!(len, 4);
        assert!(!ops.is_empty(), "MIPS64 LE emitted no p-code; ops={ops:?}");
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected MIPS64 LE addu to emit INT_ADD; ops={ops:?}"
        );
    }

    // ── Z80 / Z180 ───────────────────────────────────────────────────────
    #[test]
    fn z80_lifts_nop_from_spec_template() {
        for language in ["z80:LE:16:default", "z180:LE:16:default"] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [0x00u8]; // NOP
            let decoded = frontend
                .decode_window(&bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(1), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 1, "{language}");
        }
    }

    // ── 8-bit MCU family: 8048 / 8051 / 8085 ────────────────────────────
    #[test]
    fn mcu_8bit_family_lifts_nop_from_spec_template() {
        for (language, nop_bytes, nop_len) in [
            ("8085:LE:16:default", &[0x00u8][..], 1u64),
            ("8051:BE:16:default", &[0x00u8][..], 1),
            ("80251:BE:24:default", &[0x00u8][..], 1),
            ("80390:BE:24:default", &[0x00u8][..], 1),
            ("8051:BE:24:mx51", &[0x00u8][..], 1),
            ("8048:LE:16:default", &[0x00u8][..], 1),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(nop_bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(
                decoded.first().map(|i| i.length as u64),
                Some(nop_len),
                "{language}"
            );
            let (_ops, len) = frontend
                .decode_and_lift_with_len(nop_bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, nop_len, "{language}");
        }
    }

    // ── 65C02 ─────────────────────────────────────────────────────────────
    #[test]
    fn p65c02_lifts_nop_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("65C02:LE:16:default").expect("65C02 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0xeau8]; // NOP
        let decoded = frontend
            .decode_window(&bytes, 0x8000, 1)
            .expect("65C02 NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(1));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x8000)
            .expect("65C02 NOP lift");
        assert_eq!(len, 1);
    }

    // ── 68000 family (68020 / 68030 / ColdFire) ──────────────────────────
    #[test]
    fn m68000_family_lifts_nop_from_spec_template() {
        for language in [
            "68000:BE:32:MC68020",
            "68000:BE:32:MC68030",
            "68000:BE:32:Coldfire",
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [0x4e, 0x71u8]; // NOP
            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(2), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 2, "{language}");
        }
    }

    // ── AVR extended / xmega ─────────────────────────────────────────────
    #[test]
    fn avr_extended_variants_lift_nop_from_spec_template() {
        for language in ["avr8:LE:16:extended", "avr8:LE:24:xmega"] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [0x00u8, 0x00]; // NOP = 0x0000
            let decoded = frontend
                .decode_window(&bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(2), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 2, "{language}");
        }
    }

    // ── SuperH (SH-1 / SH-2 / SH-2A) and SuperH4 ────────────────────────
    #[test]
    fn superh_family_lifts_nop_from_spec_template() {
        for (language, nop_bytes) in [
            ("SuperH:BE:32:SH-1", [0x00u8, 0x09]),
            ("SuperH:BE:32:SH-2", [0x00, 0x09]),
            ("SuperH:BE:32:SH-2A", [0x00, 0x09]),
            ("SuperH4:BE:32:default", [0x00, 0x09]),
            ("SuperH4:LE:32:default", [0x09, 0x00]),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(&nop_bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(2), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&nop_bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 2, "{language}");
        }
    }

    // ── TI MSP430 / MSP430X ───────────────────────────────────────────────
    #[test]
    fn ti_msp430_lifts_nop_from_spec_template() {
        for language in ["TI_MSP430:LE:16:default", "TI_MSP430X:LE:32:default"] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            // NOP = MOV R3,R3 = 0x4303 (LE bytes: 03 43)
            let bytes = [0x03u8, 0x43];
            let decoded = frontend
                .decode_window(&bytes, 0x8000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(2), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x8000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 2, "{language}");
        }
    }

    // ── Dalvik (representative variants) ─────────────────────────────────
    #[test]
    fn dalvik_base_lifts_nop_from_spec_template() {
        for language in [
            "Dalvik:LE:32:default",
            "Dalvik:LE:32:DEX_Android10",
            "Dalvik:LE:32:DEX_Android12",
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            // Dalvik NOP = 0x0000 (2 bytes), but Dalvik code-unit length = 1 (1 code unit = 2 bytes)
            let bytes = [0x00u8, 0x00, 0x00, 0x00]; // extra bytes for window
            let decoded = frontend
                .decode_window(&bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            // length=1 means 1 Dalvik code unit (= 2 bytes); decode_and_lift returns byte count
            assert!(
                decoded.first().is_some(),
                "{language} should decode something"
            );
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert!(len > 0, "{language} lift length must be positive");
        }
    }

    // ── eBPF (LE/BE) ─────────────────────────────────────────────────────
    #[test]
    fn ebpf_lifts_mov_from_spec_template() {
        for language in ["eBPF:LE:64:default", "eBPF:BE:64:default"] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            // MOV r0, r0 (eBPF ALU64 MOV BPF_X): opcode=0xBF, regs=0x00, off=0, imm=0
            let bytes = [0xbf, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            let decoded = frontend
                .decode_window(&bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} MOV decode: {e}"));
            assert!(decoded.first().map(|i| i.length).is_some(), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} MOV lift: {e}"));
            assert!(len > 0, "{language} lift length must be positive");
        }
    }

    // ── Classic BPF (32-bit) ──────────────────────────────────────────────
    #[test]
    fn bpf_le_lifts_ld_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("BPF:LE:32:default").expect("BPF LE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        // Classic BPF: LD A, #0 = {op=0x00, jt=0, jf=0, k=0} (8 bytes)
        let bytes = [0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x0000, 1)
            .expect("BPF LD decode");
        assert!(
            decoded.first().map(|i| i.length).is_some(),
            "BPF should decode 8-byte instruction"
        );
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x0000)
            .expect("BPF LD lift");
        assert!(len > 0, "BPF LD lift length must be positive");
    }

    // ── V850 ─────────────────────────────────────────────────────────────
    #[test]
    fn v850_lifts_nop_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("V850:LE:32:default").expect("V850 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0x00u8, 0x00]; // NOP (format I: opcode=0, r1=0, r2=0)
        let decoded = frontend
            .decode_window(&bytes, 0x1000, 1)
            .expect("V850 NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(2));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x1000)
            .expect("V850 NOP lift");
        assert_eq!(len, 2);
    }

    // ── MC6800 family: 6809 / H6309 / 6805 ─────────────────────────────
    #[test]
    fn mc6800_family_lifts_nop_from_spec_template() {
        for (language, nop_byte, nop_len) in [
            ("6809:BE:16:default", 0x12u8, 1u64), // NOP = 0x12
            ("H6309:BE:16:default", 0x12, 1),     // compatible
            ("6805:BE:16:default", 0x9d, 1),      // NOP = 0x9D
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [nop_byte];
            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(
                decoded.first().map(|i| i.length as u64),
                Some(nop_len),
                "{language}"
            );
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, nop_len, "{language}");
        }
    }

    // ── HCS08 family: HC05 / HC08 / HCS08 ────────────────────────────────
    #[test]
    fn hcs08_family_lifts_nop_from_spec_template() {
        for language in [
            "HC05:BE:16:default",
            "HC08:BE:16:default",
            "HCS08:BE:16:default",
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [0x9du8]; // NOP = 0x9D
            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(1), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 1, "{language}");
        }
    }

    // ── HCS12 family: HC12 / HCS12 / HCS12X ──────────────────────────────
    #[test]
    fn hcs12_family_lifts_nop_from_spec_template() {
        for language in [
            "HC-12:BE:16:default",
            "HCS-12:BE:24:default",
            "HCS-12X:BE:24:default",
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let bytes = [0xa7u8]; // NOP = 0xA7
            let decoded = frontend
                .decode_window(&bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(1), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 1, "{language}");
        }
    }

    // ── JVM ──────────────────────────────────────────────────────────────
    #[test]
    fn jvm_resolves_as_executable_candidate() {
        // JVM requires specific packed context initialization that is not trivially
        // exercisable without a full .class file header. Verify registry resolution only.
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        let selection = registry
            .resolve_from_language_pair("JVM:BE:32:default", None)
            .expect("JVM resolve");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    // ── MCS96 ────────────────────────────────────────────────────────────
    #[test]
    fn mcs96_lifts_nop_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("MCS96:LE:16:default").expect("MCS96 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        // MCS96 NOP = 0xFD (1 byte; 0x00 = SKIP, 0x01 = CLR)
        let bytes = [0xfdu8];
        let decoded = frontend
            .decode_window(&bytes, 0x2000, 1)
            .expect("MCS96 NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(1));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x2000)
            .expect("MCS96 NOP lift");
        assert_eq!(len, 1);
    }

    // ── M16C ─────────────────────────────────────────────────────────────
    #[test]
    fn m16c_lifts_nop_from_spec_template() {
        // M16C NOP = 0x04 (1 byte for M16C/60; M16C/80 extended mode reports 2 bytes)
        for (language, nop_bytes, expected_len) in [
            ("M16C/60:LE:16:default", &[0x04u8, 0x00][..], 1u64),
            ("M16C/80:LE:16:default", &[0x04u8, 0x00][..], 2u64),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(nop_bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(
                decoded.first().map(|i| i.length as u64),
                Some(expected_len),
                "{language}"
            );
            let (_ops, len) = frontend
                .decode_and_lift_with_len(nop_bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, expected_len, "{language}");
        }
    }

    // ── M8C ──────────────────────────────────────────────────────────────
    #[test]
    fn m8c_lifts_nop_from_spec_template() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("M8C:BE:16:default").expect("M8C runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let bytes = [0x00u8]; // NOP = 0x00
        let decoded = frontend
            .decode_window(&bytes, 0x0000, 1)
            .expect("M8C NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(1));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x0000)
            .expect("M8C NOP lift");
        assert_eq!(len, 1);
    }

    // ── PA-RISC ──────────────────────────────────────────────────────────
    #[test]
    fn pa_risc_resolves_as_executable_candidate() {
        // PA-RISC SLA has a zero-length terminal constraint for standalone instructions
        // without a full program context; verify registry resolution is correct.
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        let selection = registry
            .resolve_from_language_pair("pa-risc:BE:32:default", None)
            .expect("PA-RISC resolve");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    // ── NDS32 (BE/LE) ────────────────────────────────────────────────────
    #[test]
    fn nds32_lifts_nop_from_spec_template() {
        // NDS32 NOP16 (16-bit compact): I16=0, opc6=0b001001, rt4=0, imm5u=0
        // bits[15:0]: [15]=0, [14:9]=001001, [8:5]=0, [4:0]=0 = 0x1200
        // BE: 0x12 0x00; LE: 0x00 0x12
        // The Fission decoder reports length=4 (aligned decode unit) for NDS32
        for (language, nop_bytes) in [
            ("NDS32:BE:32:default", [0x12u8, 0x00, 0x00, 0x00]),
            ("NDS32:LE:32:default", [0x00u8, 0x12, 0x00, 0x00]),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(&nop_bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            // NOP16 decodes as 4 bytes aligned unit in Fission's NDS32 decoder
            assert_eq!(decoded.first().map(|i| i.length), Some(4), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&nop_bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 4, "{language}");
        }
    }

    // ── Xtensa (LE only; BE SLA has different decode constraints) ────────
    #[test]
    fn xtensa_le_lifts_nop_from_spec_template() {
        // Xtensa NOP (RRR format, 24-bit instruction):
        // op2=0, r=2(ar), s=0(as), t=0xF(at=15), op1=0, op0=0
        // RRR layout: bits[23:20]=op2, [19:16]=r, [15:12]=s, [11:8]=t, [7:4]=op1, [3:0]=op0
        // val = (2<<16) | (15<<8) = 0x020F00
        // LE 3-byte store: byte0=bits[7:0]=0x00, byte1=bits[15:8]=0x0F, byte2=bits[23:16]=0x02
        let frontend = RuntimeSleighFrontend::new_for_language("Xtensa:LE:32:default")
            .expect("Xtensa LE runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        let nop_bytes = [0x00u8, 0x0f, 0x02];
        let decoded = frontend
            .decode_window(&nop_bytes, 0x1000, 1)
            .expect("Xtensa LE NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(3));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&nop_bytes, 0x1000)
            .expect("Xtensa LE NOP lift");
        assert_eq!(len, 3);
    }

    #[test]
    fn xtensa_be_resolves_as_executable_candidate() {
        // Xtensa BE has different SLA decode constraints from the LE variant;
        // confirm registry registration is correct.
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        let selection = registry
            .resolve_from_language_pair("Xtensa:BE:32:default", None)
            .expect("Xtensa BE resolve");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    // ── TriCore ───────────────────────────────────────────────────────────
    #[test]
    fn tricore_lifts_nop_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("tricore:LE:32:default")
            .expect("TriCore runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        // NOP (SR format): op0007=0x0, op0815=0x0 → LE 2 bytes: 0x00 0x00
        let bytes = [0x00u8, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x80000000, 1)
            .expect("TriCore NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(2));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x80000000)
            .expect("TriCore NOP lift");
        assert_eq!(len, 2);
    }

    // ── PIC family (representative: PIC-16, PIC-18, PIC-24E, dsPIC33E) ───
    #[test]
    fn pic_family_lifts_nop_from_spec_template() {
        // PIC-12/16: 14-bit word, stored as 2 bytes; NOP = 0x0000
        // PIC-18: 16-bit word (2 bytes); NOP = 0x0000
        // PIC-24/dsPIC: 24-bit instruction words, but address space is word-addressed
        //   and the Ghidra decoder needs a 4-byte aligned window for 24-bit instructions.
        //   NOP24 = 0x000000, with phantom byte → 4 bytes in window.
        for (language, nop_bytes, nop_len) in [
            ("PIC-12:LE:16:PIC-12C5xx", &[0x00u8, 0x00][..], 2u64),
            ("PIC-16:LE:16:PIC-16", &[0x00u8, 0x00][..], 2),
            ("PIC-16:LE:16:PIC-16F", &[0x00u8, 0x00][..], 2),
            ("PIC-18:LE:24:PIC-18", &[0x00u8, 0x00, 0x00, 0x00][..], 2),
            ("PIC-24E:LE:24:default", &[0x00u8, 0x00, 0x00, 0x00][..], 4),
            ("PIC-24F:LE:24:default", &[0x00u8, 0x00, 0x00, 0x00][..], 4),
            ("dsPIC33E:LE:24:default", &[0x00u8, 0x00, 0x00, 0x00][..], 4),
            ("dsPIC33F:LE:24:default", &[0x00u8, 0x00, 0x00, 0x00][..], 4),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(nop_bytes, 0x0000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(
                decoded.first().map(|i| i.length as u64),
                Some(nop_len),
                "{language}"
            );
            let (_ops, len) = frontend
                .decode_and_lift_with_len(nop_bytes, 0x0000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, nop_len, "{language}");
        }
    }

    // ── PowerPC 4xx / e500 / e500mc / QUICC / VLE ────────────────────────
    #[test]
    fn powerpc_extended_variants_lift_nop_from_spec_template() {
        for (language, nop_bytes) in [
            ("PowerPC:BE:32:4xx", [0x60u8, 0x00, 0x00, 0x00]),
            ("PowerPC:LE:32:4xx", [0x00u8, 0x00, 0x00, 0x60]),
            ("PowerPC:BE:32:e500", [0x60u8, 0x00, 0x00, 0x00]),
            ("PowerPC:LE:32:e500", [0x00u8, 0x00, 0x00, 0x60]),
            ("PowerPC:BE:32:e500mc", [0x60u8, 0x00, 0x00, 0x00]),
            ("PowerPC:LE:32:e500mc", [0x00u8, 0x00, 0x00, 0x60]),
            ("PowerPC:BE:32:MPC8270", [0x60u8, 0x00, 0x00, 0x00]),
            ("PowerPC:LE:32:QUICC", [0x00u8, 0x00, 0x00, 0x60]),
        ] {
            let frontend = RuntimeSleighFrontend::new_for_language(language)
                .unwrap_or_else(|e| panic!("{language} runtime: {e}"));
            assert_eq!(
                frontend.status(),
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
            let decoded = frontend
                .decode_window(&nop_bytes, 0x1000, 1)
                .unwrap_or_else(|e| panic!("{language} NOP decode: {e}"));
            assert_eq!(decoded.first().map(|i| i.length), Some(4), "{language}");
            let (_ops, len) = frontend
                .decode_and_lift_with_len(&nop_bytes, 0x1000)
                .unwrap_or_else(|e| panic!("{language} NOP lift: {e}"));
            assert_eq!(len, 4, "{language}");
        }
    }

    // ── SPARC V9 32-bit ──────────────────────────────────────────────────
    #[test]
    fn sparc_v9_32_lifts_sethi_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("sparc:BE:32:default")
            .expect("SPARC V9 32 runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        // sethi %hi(0), %g0 = NOP = 0x01000000
        let bytes = [0x01u8, 0x00, 0x00, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x1000, 1)
            .expect("SPARC V9 32 NOP decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(4));
        let (_ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x1000)
            .expect("SPARC V9 32 NOP lift");
        assert_eq!(len, 4);
    }

    // ── RISC-V AndesStar ─────────────────────────────────────────────────
    #[test]
    fn riscv_andestar_lifts_addi_from_spec_template() {
        let frontend = RuntimeSleighFrontend::new_for_language("RISCV:LE:32:AndeStar_v5")
            .expect("RISC-V AndesStar runtime");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::ExecutableCandidate
        );
        // addi x10, x10, 1 → 0x00150513 LE: 13 05 15 00
        let bytes = [0x13u8, 0x05, 0x15, 0x00];
        let decoded = frontend
            .decode_window(&bytes, 0x10000, 1)
            .expect("RISC-V AndesStar addi decode");
        assert_eq!(decoded.first().map(|i| i.length), Some(4));
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x10000)
            .expect("RISC-V AndesStar addi lift");
        assert_eq!(len, 4);
        assert!(
            !ops.is_empty(),
            "RISC-V AndesStar addi emitted no p-code; ops={ops:?}"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "expected RISC-V AndesStar addi to emit INT_ADD; ops={ops:?}"
        );
    }

    // ── Toy architectures (internal test arches) ──────────────────────────
    #[test]
    fn toy_architectures_resolve_as_executable_candidate() {
        // Toy architectures are Ghidra-internal test targets.
        // Verify they resolve correctly in the registry without requiring lift.
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        for language in [
            "Toy:BE:64:default",
            "Toy:LE:64:default",
            "Toy:BE:32:default",
            "Toy:LE:32:default",
            "Toy:BE:32:posStack",
            "Toy:BE:32:builder",
            "Toy:LE:32:builder",
        ] {
            let selection = registry
                .resolve_from_language_pair(language, None)
                .unwrap_or_else(|e| panic!("{language} resolve: {e}"));
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
        }
    }

    // ── DATA (pseudo-architecture) ────────────────────────────────────────
    #[test]
    fn data_architectures_resolve_as_executable_candidate() {
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        for language in ["DATA:BE:64:default", "DATA:LE:64:default"] {
            let selection = registry
                .resolve_from_language_pair(language, None)
                .unwrap_or_else(|e| panic!("{language} resolve: {e}"));
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
        }
    }

    // ── CP1600 / CR16 ─────────────────────────────────────────────────────
    #[test]
    fn cp1600_and_cr16_resolve_as_executable_candidate() {
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        for language in [
            "CP1600:BE:16:default",
            "CR16AB:LE:16:default",
            "CR16C:LE:16:default",
        ] {
            let selection = registry
                .resolve_from_language_pair(language, None)
                .unwrap_or_else(|e| panic!("{language} resolve: {e}"));
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate,
                "{language}"
            );
        }
    }

    // ── AVR32 ─────────────────────────────────────────────────────────────
    #[test]
    fn avr32_resolves_as_executable_candidate() {
        let registry = CompiledRuntimeRegistry::discover().expect("registry");
        let selection = registry
            .resolve_from_language_pair("avr32:BE:32:default", None)
            .expect("avr32 resolve");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    #[test]
    fn cfg_blocks_split_nonconstant_direct_branch_target() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::Copy,
                Some(var(0x10, 4)),
                vec![Varnode::constant(1, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::CBranch,
                None,
                vec![var(0x110, 8), Varnode::constant(1, 1)],
            ),
            op(
                2,
                0x108,
                PcodeOpcode::Copy,
                Some(var(0x20, 4)),
                vec![Varnode::constant(2, 4)],
            ),
            op(3, 0x110, PcodeOpcode::Return, None, vec![]),
        ];

        let blocks = build_cfg_blocks_from_ops(0x100, ops, &BTreeSet::new());
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[1].start_address, 0x108);
        assert_eq!(blocks[2].start_address, 0x110);
        assert_eq!(blocks[0].successors, vec![1, 2]);
    }

    #[test]
    fn winmain_crt_startup_lift_includes_call_fallthrough() {
        use fission_loader::loader::LoadedBinary;
        use std::path::Path;

        let binary_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark/binary/x86-64/window/small/binary/c/test_functions.exe");
        let binary = LoadedBinary::from_file(&binary_path).expect("load binary");
        let load_spec = binary.load_spec().expect("load spec");
        let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)
            .expect("frontends");
        let frontend = frontends.first().expect("frontend");
        let entry = 0x1400013e0u64;
        let max_bytes = {
            let inner = binary.inner();
            let mut next = entry.saturating_add(256 * 1024);
            for info in &inner.functions {
                if info.address > entry && info.address < next {
                    next = info.address;
                }
            }
            next.saturating_sub(entry) as usize
        };
        let bytes = binary.view_bytes(entry, max_bytes).expect("bytes");
        let memory_context = DecodeMemoryContext {
            block_entry_hints: binary
                .cfg_block_entry_hints_in_range(entry, entry.saturating_add(max_bytes as u64)),
            ..DecodeMemoryContext::default()
        };
        let lifted = frontend
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                entry,
                DecodeContract::decomp_function(512),
                &memory_context,
                None,
            )
            .expect("lift WinMainCRTStartup");

        assert!(
            lifted
                .reachable_instruction_addresses
                .contains(&0x1400013f7),
            "expected fallthrough after call; reachable={:?}",
            lifted
                .reachable_instruction_addresses
                .iter()
                .map(|addr| format!("0x{addr:x}"))
                .collect::<Vec<_>>()
        );
        assert!(
            lifted
                .function
                .blocks
                .iter()
                .any(|block| block.start_address == 0x1400013f7),
            "expected block leader at post-call nop; blocks={:?}",
            lifted
                .function
                .blocks
                .iter()
                .map(|block| format!("0x{:x}", block.start_address))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_atomic_instruction_cfg_matches_single_block() {
        use fission_loader::loader::LoadedBinary;
        use std::path::Path;

        let binary_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark/binary/x86-64/window/small/binary/c/instruction_matrix.exe");
        let binary = LoadedBinary::from_file(&binary_path).expect("load binary");
        let load_spec = binary.load_spec().expect("load spec");
        let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)
            .expect("frontends");
        let frontend = frontends.first().expect("frontend");
        let entry = 0x1400014d0u64;
        let max_bytes = 32usize;
        let bytes = binary.view_bytes(entry, max_bytes).expect("bytes");
        let lifted = frontend
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                entry,
                DecodeContract::decomp_function(32),
                &DecodeMemoryContext::default(),
                None,
            )
            .expect("lift test_atomic");

        assert_eq!(
            lifted.function.blocks.len(),
            1,
            "cmpxchg internal CBranch must not split BBM blocks; blocks={:?}",
            lifted
                .function
                .blocks
                .iter()
                .map(|block| format!("0x{:x}", block.start_address))
                .collect::<Vec<_>>()
        );
        assert!(
            lifted.function.blocks[0].successors.is_empty(),
            "expected exit block without successors"
        );
    }
}
