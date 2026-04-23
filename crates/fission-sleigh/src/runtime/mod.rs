mod processors;
mod spine;

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use serde::{Deserialize, Serialize};

use crate::compiler::{
    compile_frontend_for_entry_spec, discover_all_entry_specs, CompiledFrontend, EntrySpec,
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
        }
    }
}

impl std::error::Error for RuntimeSleighError {}

#[derive(Debug, Clone)]
pub struct RuntimeSleighFrontend {
    language: String,
    entry: EntrySpec,
    status: RuntimeFrontendStatus,
    compiled: Option<CompiledFrontend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFrontendDescriptor {
    pub arch: String,
    pub processor: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_ids: Vec<String>,
    pub compatibility_aliases: Vec<String>,
    pub generated_path: String,
    pub status: RuntimeFrontendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRuntimeRegistry {
    frontends: Vec<RuntimeFrontendDescriptor>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedInstruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub length: usize,
    pub mnemonic: String,
    pub operands_text: String,
    pub flow_kind: DecodedFlowKind,
    pub direct_target: Option<u64>,
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

impl CompiledRuntimeRegistry {
    pub fn discover() -> Result<Self> {
        let frontends = discover_all_entry_specs()?
            .into_iter()
            .map(|entry| RuntimeFrontendDescriptor {
                generated_path: format!("{}/{}", entry.arch, entry.entry_id),
                status: status_for_entry(&entry),
                processor: entry.arch.clone(),
                arch: entry.arch,
                entry_spec: entry.entry_spec,
                entry_id: entry.entry_id,
                language_ids: entry.language_ids,
                compatibility_aliases: entry.compatibility_aliases,
            })
            .collect::<Vec<_>>();
        validate_processor_skeleton_coverage(&frontends);
        Ok(Self { frontends })
    }

    pub fn frontends(&self) -> &[RuntimeFrontendDescriptor] {
        &self.frontends
    }

    pub fn lookup(&self, language_name: &str) -> Option<&RuntimeFrontendDescriptor> {
        self.frontends.iter().find(|frontend| {
            frontend.entry_id == language_name
                || frontend.entry_spec == format!("{language_name}.slaspec")
                || frontend.processor == language_name
                || frontend.entry_id.eq_ignore_ascii_case(language_name)
                || frontend.processor.eq_ignore_ascii_case(language_name)
                || frontend
                    .language_ids
                    .iter()
                    .any(|id| id == language_name || id.eq_ignore_ascii_case(language_name))
                || frontend.compatibility_aliases.iter().any(|alias| {
                    alias == language_name || alias.eq_ignore_ascii_case(language_name)
                })
        })
    }
}

fn validate_processor_skeleton_coverage(frontends: &[RuntimeFrontendDescriptor]) {
    let manifest_processors = frontends
        .iter()
        .map(|frontend| frontend.processor.as_str())
        .collect::<BTreeSet<_>>();
    let skeleton_processors = processors::PROCESSOR_SKELETONS
        .iter()
        .map(|skeleton| skeleton.ghidra_processor)
        .collect::<BTreeSet<_>>();
    debug_assert_eq!(skeleton_processors, manifest_processors);
    debug_assert!(processors::PROCESSOR_SKELETONS
        .iter()
        .all(|skeleton| !skeleton.module_name.is_empty()));
}

fn entry_matches_language_name(entry: &EntrySpec, language_name: &str) -> bool {
    entry.entry_id == language_name
        || entry.entry_spec == format!("{language_name}.slaspec")
        || entry.entry_id.eq_ignore_ascii_case(language_name)
        || entry.arch.eq_ignore_ascii_case(language_name)
        || entry
            .language_ids
            .iter()
            .any(|id| id == language_name || id.eq_ignore_ascii_case(language_name))
        || entry
            .compatibility_aliases
            .iter()
            .any(|alias| alias == language_name || alias.eq_ignore_ascii_case(language_name))
}

impl RuntimeSleighFrontend {
    pub fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs/languages")
    }

    pub fn find_spec_path_for(language_name: &str) -> Option<PathBuf> {
        discover_all_entry_specs()
            .ok()?
            .into_iter()
            .find(|entry| entry_matches_language_name(entry, language_name))
            .map(|entry| entry.path)
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::find_spec_path_for(language_name)
            .unwrap_or_else(|| Self::spec_dir().join(format!("{}.slaspec", language_name)))
    }

    pub fn new_for_language(language_name: &str) -> Result<Self> {
        let entry = discover_all_entry_specs()?
            .into_iter()
            .find(|entry| entry_matches_language_name(entry, language_name))
            .ok_or_else(|| {
                anyhow!("Sleigh runtime frontend not registered for '{language_name}'")
            })?;
        let status = status_for_entry(&entry);
        let compiled = if status == RuntimeFrontendStatus::ExecutableCandidate {
            Some(compile_frontend_for_entry_spec(&entry.path)?)
        } else {
            None
        };
        Ok(Self {
            language: language_name.to_string(),
            entry,
            status,
            compiled,
        })
    }

    pub fn new(spec_path: &Path) -> Result<Self> {
        let language = spec_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow!("Invalid Sleigh spec path: {}", spec_path.display()))?;
        Self::new_for_language(language)
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn entry(&self) -> &EntrySpec {
        &self.entry
    }

    pub fn status(&self) -> RuntimeFrontendStatus {
        self.status
    }

    pub fn compile_language_runtime(&self) -> Result<LanguageRuntime> {
        LanguageRuntime::compile(&self.entry)
    }

    pub fn runtime_attempt_report(&self) -> Result<RuntimeAttemptReport> {
        Ok(self.compile_language_runtime()?.attempt_report())
    }

    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    pub fn decode_and_lift_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => {
                if self.entry.arch == "x86" && self.entry.entry_id == "x86-64" {
                    processors::x86::generated::decode_and_lift(
                        self.compiled.as_ref().ok_or_else(|| {
                            anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                        })?,
                        bytes,
                        address,
                    )
                } else {
                    Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                        language: self.entry.entry_id.clone(),
                        reason: "executable candidate has no runtime consumer".to_string(),
                    }
                    .into())
                }
            }
        }
    }

    pub fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        if limit == 0 || bytes.is_empty() {
            return Ok(Vec::new());
        }

        let mut decoded = Vec::with_capacity(limit.min(64));
        let mut offset = 0usize;
        let mut current = address;
        while offset < bytes.len() && decoded.len() < limit {
            let remaining = &bytes[offset..];
            let instruction = match self.decode_instruction_with_len(remaining, current) {
                Ok(instruction) => instruction,
                Err(err) if decoded.is_empty() => return Err(err),
                Err(_) => break,
            };
            if instruction.length == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = instruction.length;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }
            current = current.saturating_add(step as u64);
            offset = offset.saturating_add(step);
            decoded.push(instruction);
        }
        Ok(decoded)
    }

    pub fn discover_direct_call_targets(
        &self,
        bytes: &[u8],
        base_address: u64,
    ) -> Result<Vec<u64>> {
        let mut targets = BTreeSet::new();
        let mut offset = 0usize;
        let mut current = base_address;
        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            let instruction = match self.decode_instruction_with_len(remaining, current) {
                Ok(instruction) => instruction,
                Err(err) if offset == 0 => return Err(err),
                Err(_) => break,
            };
            if instruction.flow_kind == DecodedFlowKind::Call {
                if let Some(target) = instruction.direct_target {
                    targets.insert(target);
                }
            }
            if instruction.length == 0 || instruction.length > remaining.len() {
                break;
            }
            current = current.saturating_add(instruction.length as u64);
            offset = offset.saturating_add(instruction.length);
        }
        Ok(targets.into_iter().collect())
    }

    fn decode_instruction_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => {
                if self.entry.arch == "x86" && self.entry.entry_id == "x86-64" {
                    processors::x86::generated::decode_instruction(
                        self.compiled.as_ref().ok_or_else(|| {
                            anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                        })?,
                        bytes,
                        address,
                    )
                } else {
                    Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                        language: self.entry.entry_id.clone(),
                        reason: "executable candidate has no runtime consumer".to_string(),
                    }
                    .into())
                }
            }
        }
    }

    pub fn lift_raw_pcode_function(
        &self,
        bytes: &[u8],
        entry_address: u64,
    ) -> Result<PcodeFunction> {
        Ok(self
            .lift_raw_pcode_function_with_contract(
                bytes,
                entry_address,
                DEFAULT_FUNCTION_INSTRUCTION_LIMIT,
            )?
            .function)
    }

    pub fn lift_raw_pcode_function_with_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        instruction_limit: usize,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract(
            bytes,
            entry_address,
            DecodeContract::strict_function(instruction_limit),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
    ) -> Result<DecodedPcodeFunction> {
        if bytes.is_empty() {
            bail!("No function bytes available at 0x{:x}", entry_address);
        }
        if contract.instruction_limit == 0 {
            bail!("instruction_limit must be > 0");
        }

        let mut ops = Vec::new();
        let mut offset = 0usize;
        let mut current = entry_address;
        let mut global_seq = 0u32;
        let mut instruction_count = 0usize;
        let mut stop_reason = DecodeStopReason::InputExhausted;

        while offset < bytes.len() && instruction_count < contract.instruction_limit {
            let remaining = &bytes[offset..];
            let (mut ins_ops, decoded_len) = self
                .decode_and_lift_with_len(remaining, current)
                .map_err(|err| anyhow!("decode failed at 0x{:x}: {:#}", current, err))?;

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = usize::try_from(decoded_len)?;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            for op in &mut ins_ops {
                op.seq_num = global_seq;
                global_seq = global_seq.saturating_add(1);
            }
            let terminates = ins_ops
                .last()
                .map(|op| contract.is_terminal_control_flow(op.opcode))
                .unwrap_or(false);

            ops.extend(ins_ops);
            offset = offset.saturating_add(step);
            current = current.saturating_add(decoded_len);
            instruction_count = instruction_count.saturating_add(1);

            if terminates {
                stop_reason = DecodeStopReason::TerminalControlFlow;
                break;
            }
        }

        if instruction_count >= contract.instruction_limit && offset < bytes.len() {
            stop_reason = DecodeStopReason::InstructionLimit;
        }
        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        Ok(DecodedPcodeFunction {
            function: PcodeFunction {
                blocks: build_cfg_blocks(entry_address, ops),
            },
            decoded_instructions: instruction_count,
            stop_reason,
        })
    }
}

fn status_for_entry(entry: &EntrySpec) -> RuntimeFrontendStatus {
    if entry.arch == "x86" && entry.entry_id == "x86-64" {
        RuntimeFrontendStatus::ExecutableCandidate
    } else {
        RuntimeFrontendStatus::RegisteredCompileOnly
    }
}

fn is_cfg_split_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch | PcodeOpcode::CBranch | PcodeOpcode::BranchInd | PcodeOpcode::Return
    )
}

fn direct_control_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op
            .inputs
            .first()
            .filter(|vn| vn.is_constant)
            .map(|vn| vn.constant_val as u64),
        _ => None,
    }
}

fn cfg_build_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
        || std::env::var_os("FISSION_PREVIEW_DEBUG").is_some()
        || std::env::var_os("FISSION_SLEIGH_CFG_DIAG").is_some()
}

fn cfg_build_diag_log(entry_address: u64, message: &str) {
    if !cfg_build_diag_enabled() {
        return;
    }
    eprintln!("[CFG-DIAG] {message}");
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some()
        || std::env::var_os("FISSION_SLEIGH_CFG_DIAG").is_some()
    {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{entry_address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, format!("[cfg-build] {message}\n").as_bytes())
            });
    }
}

fn format_varnode_diag(vn: &Varnode) -> String {
    format!(
        "space={} off=0x{:x} size={} const={} val={}",
        vn.space_id, vn.offset, vn.size, vn.is_constant, vn.constant_val
    )
}

fn push_successor(successors: &mut Vec<u32>, succ: u32) {
    if !successors.contains(&succ) {
        successors.push(succ);
    }
}

pub fn build_cfg_blocks(entry_address: u64, ops: Vec<PcodeOp>) -> Vec<PcodeBasicBlock> {
    if ops.is_empty() {
        return Vec::new();
    }

    cfg_build_diag_log(
        entry_address,
        &format!("start entry=0x{:x} op_count={}", entry_address, ops.len()),
    );

    let mut addr_to_op_idx: HashMap<u64, usize> = HashMap::new();
    for (idx, op) in ops.iter().enumerate() {
        addr_to_op_idx.entry(op.address).or_insert(idx);
    }

    let mut block_starts: BTreeSet<usize> = BTreeSet::new();
    block_starts.insert(0);

    for (idx, op) in ops.iter().enumerate() {
        if is_cfg_split_opcode(op.opcode) {
            if idx + 1 < ops.len() {
                block_starts.insert(idx + 1);
            }
            if let Some(target) = direct_control_target(op) {
                if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                    block_starts.insert(target_idx);
                }
            }
        }
    }

    let starts: Vec<usize> = block_starts.into_iter().collect();
    let mut op_to_block = vec![0u32; ops.len()];
    for (block_idx, start) in starts.iter().enumerate() {
        let end = starts.get(block_idx + 1).copied().unwrap_or(ops.len());
        for slot in &mut op_to_block[*start..end] {
            *slot = block_idx as u32;
        }
    }

    let mut blocks = Vec::with_capacity(starts.len());
    for (block_idx, start) in starts.iter().enumerate() {
        let end = starts.get(block_idx + 1).copied().unwrap_or(ops.len());
        let mut block_ops = ops[*start..end].to_vec();
        for (local_seq, op) in block_ops.iter_mut().enumerate() {
            op.seq_num = local_seq as u32;
        }

        let mut successors = Vec::new();
        let mut branch_target = None;
        let mut branch_input = None;
        if let Some(last) = block_ops.last() {
            match last.opcode {
                PcodeOpcode::Branch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) = direct_control_target(last) {
                        branch_target = Some(target);
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        }
                    }
                }
                PcodeOpcode::CBranch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) = direct_control_target(last) {
                        branch_target = Some(target);
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        }
                    }
                    if block_idx + 1 < starts.len() {
                        push_successor(&mut successors, (block_idx + 1) as u32);
                    }
                }
                PcodeOpcode::BranchInd | PcodeOpcode::Return => {}
                _ => {
                    if block_idx + 1 < starts.len() {
                        push_successor(&mut successors, (block_idx + 1) as u32);
                    }
                }
            }

            if matches!(last.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
                && successors.is_empty()
            {
                cfg_build_diag_log(
                    entry_address,
                    &format!(
                        "control_block_no_successors block_idx={} block_start=0x{:x} seq=0x{:x} opcode={:?} target={} input={}",
                        block_idx,
                        last.address,
                        last.seq_num,
                        last.opcode,
                        branch_target
                            .map(|v| format!("0x{v:x}"))
                            .unwrap_or_else(|| "<none>".to_string()),
                        branch_input.as_deref().unwrap_or("<none>")
                    ),
                );
            }
        }

        let start_address = block_ops
            .first()
            .map(|op| op.address)
            .unwrap_or(entry_address);
        blocks.push(PcodeBasicBlock {
            index: block_idx as u32,
            start_address,
            successors,
            ops: block_ops,
        });
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .expect("legacy ARM alias registered");
        assert_eq!(arm_alias.processor, "ARM");
    }

    #[test]
    fn processor_skeletons_cover_all_ghidra_processors() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        let manifest_processors = registry
            .frontends()
            .iter()
            .map(|frontend| frontend.processor.as_str())
            .collect::<BTreeSet<_>>();
        let skeleton_processors = processors::PROCESSOR_SKELETONS
            .iter()
            .map(|skeleton| skeleton.ghidra_processor)
            .collect::<BTreeSet<_>>();

        assert_eq!(manifest_processors.len(), 38);
        assert_eq!(skeleton_processors, manifest_processors);
        assert_eq!(
            processors::PROCESSOR_SKELETONS
                .iter()
                .filter(|skeleton| skeleton.executable_candidate)
                .map(|skeleton| skeleton.ghidra_processor)
                .collect::<Vec<_>>(),
            vec!["x86"]
        );
    }

    #[test]
    fn compile_only_frontend_produces_fail_closed_runtime_report() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("AARCH64").expect("AARCH64 runtime frontend");
        assert_eq!(
            frontend.status(),
            RuntimeFrontendStatus::RegisteredCompileOnly
        );

        let report = frontend
            .runtime_attempt_report()
            .expect("compile-only runtime report");
        assert_eq!(report.processor, "AARCH64");
        assert_eq!(report.module_name, "aarch64");
        assert!(report.compiled_table_available);
        assert!(report.constructor_inventory_count > 0);
        assert!(report.fail_closed_reason.is_some());

        let err = frontend
            .decode_and_lift_with_len(&[0x00, 0x00, 0x00, 0x00], 0x1000)
            .expect_err("compile-only runtime must fail closed");
        assert!(format!("{err:#}").contains("UnsupportedGeneratedSemantic"));
    }

    #[test]
    fn runtime_frontend_executes_x86_64_ret() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("x86-64 runtime frontend");
        let (ops, len) = frontend
            .decode_and_lift_with_len(&[0xC3], 0x1000)
            .expect("compiled x86-64 ret runtime");
        assert_eq!(len, 1);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::Return));
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
