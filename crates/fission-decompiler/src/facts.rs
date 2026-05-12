use crate::decode_rust_sleigh_pcode;
use crate::{
    CallEdgeKind, CallEffectSummarySource, CallTargetProvenance, CallTargetRef,
    NirCallEffectSummary, NirCallParamRule, NirCallPrototypeSummary, NirFunctionHints,
    NirRenderOptions, NirTypeContext, PcodeFunction, PcodeOpcode, infer_entry_register_param_arity,
};
use fission_core::{normalize_named_type_identity, sanitize_symbol_name};
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_signatures::SIGNATURE_RESOURCES;
use fission_signatures::win_types::WindowsStructures;
use fission_static::analysis::decomp::facts::FactProvenance;
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(crate) fn build_nir_type_context(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> NirTypeContext {
    let mut index = CallTargetIndex::default();
    let mut iat_index = CallTargetIndex::default();

    for func in binary.imports() {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        iat_index.add(func.address, &func.name, CandidateClass::Import);
    }

    for (resolved_address, name) in &binary.inner().iat_symbols {
        if *resolved_address == 0 || name.is_empty() {
            continue;
        }
        iat_index.add(*resolved_address, name, CandidateClass::Import);
    }

    for (resolved_address, fact) in fact_store.iter_resolved_name_facts() {
        if matches!(fact.provenance, FactProvenance::ImportExport) {
            continue;
        }
        if fact.name.is_empty() {
            continue;
        }
        index.add(resolved_address, &fact.name, CandidateClass::Fact);
    }

    for func in &binary.functions {
        if func.name.is_empty() {
            continue;
        }
        if func.is_import {
            continue;
        }
        let class = if func.is_export && func.is_thunk_like {
            CandidateClass::ExportThunk
        } else if func.is_export {
            CandidateClass::Export
        } else {
            CandidateClass::Direct
        };
        index.add(func.address, &func.name, class);
        if func.is_export
            && func.is_thunk_like
            && let Some(thunk_target) = func.thunk_target
            && thunk_target != 0
        {
            index.add(thunk_target, &func.name, CandidateClass::ExportThunkTarget);
        }
    }

    for (resolved_address, name) in &binary.inner().global_symbols {
        if name.is_empty() {
            continue;
        }
        index.add(*resolved_address, name, CandidateClass::Global);
    }
    let resolved_index = index.finish();
    let resolved_iat_index = iat_index.finish();
    let call_target_refs = resolved_index.call_target_refs;
    let iat_target_refs = resolved_iat_index.call_target_refs;
    let call_targets = call_target_refs
        .iter()
        .chain(iat_target_refs.iter())
        .map(|(address, target_ref)| (*address, target_ref.symbol.clone()))
        .collect::<HashMap<_, _>>();
    let all_target_refs = call_target_refs
        .iter()
        .chain(iat_target_refs.iter())
        .map(|(address, target_ref)| (*address, target_ref.clone()))
        .collect::<HashMap<_, _>>();

    NirTypeContext {
        call_targets,
        call_target_refs: call_target_refs.clone(),
        iat_target_refs: iat_target_refs.clone(),
        ambiguous_call_targets: resolved_index.ambiguous_call_targets,
        call_effect_summaries: build_nir_call_effect_summaries(&all_target_refs),
        call_prototype_summaries: HashMap::new(),
        call_param_rules: build_nir_call_param_rules(&all_target_refs),
        function_hints: build_nir_function_hints(fact_store, address),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateClass {
    Import,
    Fact,
    ExportThunk,
    ExportThunkTarget,
    Export,
    Direct,
    Global,
}

impl CandidateClass {
    fn rank(self) -> u8 {
        match self {
            CandidateClass::Import => 7,
            CandidateClass::ExportThunk | CandidateClass::ExportThunkTarget => 6,
            CandidateClass::Export => 5,
            CandidateClass::Fact => 4,
            CandidateClass::Direct => 3,
            CandidateClass::Global => 2,
        }
    }

    fn provenance(self) -> CallTargetProvenance {
        match self {
            CandidateClass::Import => CallTargetProvenance::Import,
            CandidateClass::Fact => CallTargetProvenance::Fact,
            CandidateClass::ExportThunk => CallTargetProvenance::Export,
            CandidateClass::ExportThunkTarget => CallTargetProvenance::ExportThunkTarget,
            CandidateClass::Export => CallTargetProvenance::Export,
            CandidateClass::Direct => CallTargetProvenance::Direct,
            CandidateClass::Global => CallTargetProvenance::Global,
        }
    }

    fn edge_kind(self) -> CallEdgeKind {
        match self {
            CandidateClass::Import => CallEdgeKind::Import,
            CandidateClass::Direct => CallEdgeKind::Direct,
            _ => CallEdgeKind::Reference,
        }
    }

    fn confidence(self) -> u8 {
        match self {
            CandidateClass::Import | CandidateClass::Fact => 255,
            CandidateClass::ExportThunk | CandidateClass::ExportThunkTarget => 240,
            CandidateClass::Export => 232,
            CandidateClass::Direct => 224,
            CandidateClass::Global => 192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallTargetCandidate {
    symbol: String,
    class: CandidateClass,
}

#[derive(Debug, Default)]
struct CallTargetIndex {
    candidates: BTreeMap<u64, Vec<CallTargetCandidate>>,
}

#[derive(Debug, Default)]
struct ResolvedCallTargetIndex {
    call_target_refs: HashMap<u64, CallTargetRef>,
    ambiguous_call_targets: HashSet<u64>,
}

impl CallTargetIndex {
    fn add(&mut self, address: u64, name: &str, class: CandidateClass) {
        let sanitized = sanitize_nir_symbol_name(name);
        if sanitized.is_empty() || is_generic_loader_symbol(&sanitized) {
            return;
        }
        self.candidates
            .entry(address)
            .or_default()
            .push(CallTargetCandidate {
                symbol: sanitized,
                class,
            });
    }

    fn finish(self) -> ResolvedCallTargetIndex {
        let mut resolved = ResolvedCallTargetIndex::default();
        for (address, mut candidates) in self.candidates {
            candidates.sort_by(|left, right| {
                right
                    .class
                    .rank()
                    .cmp(&left.class.rank())
                    .then_with(|| left.symbol.cmp(&right.symbol))
            });
            let Some(best) = candidates.first() else {
                continue;
            };
            let best_rank = best.class.rank();
            let same_rank_symbols = candidates
                .iter()
                .filter(|candidate| candidate.class.rank() == best_rank)
                .map(|candidate| candidate.symbol.as_str())
                .collect::<BTreeSet<_>>();
            if same_rank_symbols.len() > 1 {
                resolved.ambiguous_call_targets.insert(address);
                continue;
            }
            resolved.call_target_refs.insert(
                address,
                CallTargetRef {
                    address: Some(address),
                    symbol: best.symbol.clone(),
                    provenance: best.class.provenance(),
                    edge_kind: best.class.edge_kind(),
                    confidence: best.class.confidence(),
                },
            );
        }
        resolved
    }
}

fn is_generic_loader_symbol(name: &str) -> bool {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    is_generic_symbol_with_prefix(stripped, "sub_")
        || is_generic_symbol_with_prefix(stripped, "FUN_0x")
        || is_generic_symbol_with_prefix(stripped, "FUN_")
        || is_generic_symbol_with_prefix(stripped, "ltmp")
        || is_generic_symbol_with_prefix(stripped, "tmp_")
}

fn is_generic_symbol_with_prefix(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn build_nir_call_effect_summaries(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> HashMap<String, NirCallEffectSummary> {
    call_target_refs
        .values()
        .map(|target_ref| {
            (
                target_ref.symbol.clone(),
                NirCallEffectSummary {
                    reads_memory: None,
                    writes_memory: None,
                    escapes_args: None,
                    may_call_unknown: None,
                    may_exit: None,
                    source: Some(CallEffectSummarySource::CallTargetRef),
                },
            )
        })
        .collect()
}

pub(crate) fn refine_nir_type_context_with_callee_effect_summaries(
    binary: &LoadedBinary,
    pcode: &PcodeFunction,
    type_context: &mut NirTypeContext,
) {
    let direct_callees = collect_direct_internal_callee_targets(pcode);
    for target_addr in direct_callees {
        let Some(target_ref) = type_context.call_target_refs.get(&target_addr) else {
            continue;
        };
        if matches!(target_ref.provenance, CallTargetProvenance::Import) {
            continue;
        }
        let Some((effect_summary, prototype_summary)) =
            build_preview_callee_summaries(binary, target_addr, &target_ref.symbol)
        else {
            continue;
        };
        type_context
            .call_effect_summaries
            .insert(target_ref.symbol.clone(), effect_summary);
        if let Some(prototype_summary) = prototype_summary {
            type_context
                .call_prototype_summaries
                .insert(target_ref.symbol.clone(), prototype_summary);
        }
    }
}

fn collect_direct_internal_callee_targets(pcode: &PcodeFunction) -> BTreeSet<u64> {
    let mut callees = BTreeSet::new();
    for block in &pcode.blocks {
        for op in &block.ops {
            if op.opcode != PcodeOpcode::Call {
                continue;
            }
            let Some(target) = op.inputs.first() else {
                continue;
            };
            callees.insert(target.offset);
        }
    }
    callees
}

fn build_preview_callee_summaries(
    binary: &LoadedBinary,
    target_addr: u64,
    target_name: &str,
) -> Option<(NirCallEffectSummary, Option<NirCallPrototypeSummary>)> {
    let function = binary.function_at_exact(target_addr)?;
    if function.is_import {
        return None;
    }
    let max_bytes = direct_callee_max_bytes(binary, target_addr)?;
    let instruction_limit = direct_callee_instruction_limit(max_bytes);
    let next_function = binary.function_after(target_addr).map(|func| func.address);
    let pcode = decode_rust_sleigh_pcode(
        binary,
        target_name,
        target_addr,
        max_bytes,
        instruction_limit,
        true,
        true,
    )
    .ok()?;
    let (summary, detail) = summarize_preview_callee_effects(&pcode);
    trace_preview_callee_effect_detail(
        target_name,
        target_addr,
        function.size,
        max_bytes,
        instruction_limit,
        next_function,
        &pcode,
        &detail,
    );
    let calling_convention = NirRenderOptions::from_loaded_binary(binary).calling_convention;
    let prototype = infer_entry_register_param_arity(&pcode, calling_convention).map(|arity| {
        NirCallPrototypeSummary {
            min_arity: arity,
            max_arity: arity,
            locked_exact_arity: Some(arity),
        }
    });
    Some((summary, prototype))
}

fn direct_callee_max_bytes(binary: &LoadedBinary, target_addr: u64) -> Option<usize> {
    let function = binary.function_at_exact(target_addr)?;
    const DEFAULT_BYTES: usize = 0x400;
    const MAX_BYTES_CAP: usize = 0x4000;

    if function.size > 0 {
        return Some((function.size as usize).min(MAX_BYTES_CAP).max(1));
    }

    if let Some(next) = binary.function_after(target_addr)
        && next.address > target_addr
    {
        let distance = (next.address - target_addr) as usize;
        return Some(distance.min(MAX_BYTES_CAP).max(1));
    }

    Some(DEFAULT_BYTES)
}

fn direct_callee_instruction_limit(max_bytes: usize) -> usize {
    let estimated = (max_bytes / 4).clamp(32, 512);
    estimated.max(32)
}

#[derive(Debug, Clone, Default)]
struct PreviewCalleeEffectDetail {
    store_count: usize,
    call_count: usize,
    callind_count: usize,
    callother_count: usize,
    return_count: usize,
    block_count: usize,
    op_count: usize,
    first_store: Option<(u64, PcodeOpcode)>,
    first_call: Option<(u64, Option<u64>, PcodeOpcode)>,
    first_callother: Option<(u64, PcodeOpcode)>,
    first_return: Option<(u64, PcodeOpcode)>,
    last_op_addr: Option<u64>,
    has_fallthrough_past_return: bool,
    is_single_call_return_wrapper: bool,
}

fn summarize_preview_callee_effects(
    pcode: &PcodeFunction,
) -> (NirCallEffectSummary, PreviewCalleeEffectDetail) {
    let mut reads_memory = Some(false);
    let mut writes_memory = Some(false);
    let mut may_call_unknown = Some(false);
    let mut may_exit = None;
    let mut saw_return = false;
    let mut detail = PreviewCalleeEffectDetail {
        block_count: pcode.blocks.len(),
        ..PreviewCalleeEffectDetail::default()
    };

    for block in &pcode.blocks {
        for op in &block.ops {
            detail.op_count += 1;
            detail.last_op_addr = Some(op.address);
            match op.opcode {
                PcodeOpcode::Load => {
                    reads_memory = Some(true);
                }
                PcodeOpcode::Store => {
                    reads_memory = Some(true);
                    writes_memory = Some(true);
                    detail.store_count += 1;
                    detail.first_store.get_or_insert((op.address, op.opcode));
                }
                PcodeOpcode::Call => {
                    may_call_unknown = Some(true);
                    detail.call_count += 1;
                    detail.first_call.get_or_insert((
                        op.address,
                        op.inputs
                            .first()
                            .and_then(|input| input.is_constant.then_some(input.offset)),
                        op.opcode,
                    ));
                }
                PcodeOpcode::CallInd => {
                    may_call_unknown = Some(true);
                    detail.callind_count += 1;
                    detail
                        .first_call
                        .get_or_insert((op.address, None, op.opcode));
                }
                PcodeOpcode::CallOther => {
                    may_call_unknown = Some(true);
                    may_exit = Some(true);
                    detail.callother_count += 1;
                    detail
                        .first_callother
                        .get_or_insert((op.address, op.opcode));
                }
                PcodeOpcode::Return => {
                    saw_return = true;
                    detail.return_count += 1;
                    detail.first_return.get_or_insert((op.address, op.opcode));
                }
                _ => {}
            }
        }
    }

    if let (Some((return_addr, _)), Some(last_op_addr)) = (detail.first_return, detail.last_op_addr)
    {
        detail.has_fallthrough_past_return = last_op_addr > return_addr;
    }
    detail.is_single_call_return_wrapper = detail.store_count == 0
        && detail.callother_count == 0
        && detail.callind_count == 0
        && detail.call_count == 1
        && detail.return_count == 1
        && detail.op_count <= 3;

    if may_exit != Some(true) {
        may_exit = if saw_return && may_call_unknown == Some(false) {
            Some(false)
        } else {
            None
        };
    }

    (
        NirCallEffectSummary {
            reads_memory,
            writes_memory,
            escapes_args: None,
            may_call_unknown,
            may_exit,
            source: Some(CallEffectSummarySource::PreviewCalleeAnalysis),
        },
        detail,
    )
}

fn trace_preview_callee_effect_detail(
    target_name: &str,
    target_addr: u64,
    function_size: u64,
    max_bytes: usize,
    instruction_limit: usize,
    next_function: Option<u64>,
    pcode: &PcodeFunction,
    detail: &PreviewCalleeEffectDetail,
) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_none() {
        return;
    }
    eprintln!(
        "[GT-TRACE] callee-lift-bounds target={} start=0x{:x} max_bytes={} instruction_limit={} function_size={} next_function={:?}",
        target_name,
        target_addr,
        max_bytes,
        instruction_limit,
        function_size,
        next_function.map(|addr| format!("0x{:x}", addr))
    );
    eprintln!(
        "[GT-TRACE] callee-effect-detail target={} target_addr=0x{:x} store_count={} call_count={} callind_count={} callother_count={} return_count={}",
        target_name,
        target_addr,
        detail.store_count,
        detail.call_count,
        detail.callind_count,
        detail.callother_count,
        detail.return_count
    );
    eprintln!(
        "[GT-TRACE] callee-shape target={} block_count={} op_count={} return_count={} has_fallthrough_past_return={} single_call_return_wrapper={}",
        target_name,
        detail.block_count,
        detail.op_count,
        detail.return_count,
        detail.has_fallthrough_past_return,
        detail.is_single_call_return_wrapper
    );
    if let Some((address, opcode)) = detail.first_store {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-store target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind=Store addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
    if let Some((address, call_target, opcode)) = detail.first_call {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-call target={} addr=0x{:x} call_target={:?} op={:?}",
            target_name, address, call_target, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind={:?} addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            opcode,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
    if let Some((address, opcode)) = detail.first_callother {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-callother target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind=CallOther addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
}

fn addr_within_function_bounds(
    address: u64,
    start_addr: u64,
    function_size: u64,
    next_function: Option<u64>,
) -> bool {
    if function_size > 0 {
        return address >= start_addr && address < start_addr.saturating_add(function_size);
    }
    if let Some(next_addr) = next_function {
        return address >= start_addr && address < next_addr;
    }
    address >= start_addr
}

fn build_nir_function_hints(fact_store: &FactStore, address: u64) -> Option<NirFunctionHints> {
    let debug = fact_store.preferred_debug_function(address)?;
    let param_names = debug
        .params
        .iter()
        .map(|param| param.name.trim().to_string())
        .collect::<Vec<_>>();
    let param_type_names = debug
        .params
        .iter()
        .enumerate()
        .filter_map(|(index, param)| {
            let type_name = param.type_name.trim();
            (!type_name.is_empty()).then(|| (index, type_name.to_string()))
        })
        .collect::<HashMap<_, _>>();
    let stack_local_names = debug
        .local_vars
        .iter()
        .filter_map(|local| match local.location {
            DwarfLocation::StackOffset(offset) if !local.name.trim().is_empty() => {
                Some((offset, local.name.trim().to_string()))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let stack_local_type_names = debug
        .local_vars
        .iter()
        .filter_map(|local| match local.location {
            DwarfLocation::StackOffset(offset) => {
                let type_name = local.type_name.trim();
                (!type_name.is_empty()).then(|| (offset, type_name.to_string()))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let return_type_name = debug
        .return_type
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned);

    if param_names.is_empty()
        && param_type_names.is_empty()
        && stack_local_names.is_empty()
        && stack_local_type_names.is_empty()
        && return_type_name.is_none()
    {
        None
    } else {
        Some(NirFunctionHints {
            param_names,
            param_type_names,
            stack_local_names,
            stack_local_type_names,
            return_type_name,
        })
    }
}

pub(crate) fn sanitize_nir_symbol_name(name: &str) -> String {
    sanitize_symbol_name(name)
}

fn build_nir_call_param_rules(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> Vec<NirCallParamRule> {
    let structures = WindowsStructures::new();
    let mut call_param_rules = Vec::new();
    let target_addresses_by_name = call_target_refs.iter().fold(
        HashMap::<String, Vec<u64>>::new(),
        |mut acc, (addr, target_ref)| {
            acc.entry(target_ref.symbol.clone())
                .or_default()
                .push(*addr);
            acc
        },
    );
    let Ok(signatures) = SIGNATURE_RESOURCES.api_signatures() else {
        return call_param_rules;
    };
    for sig in signatures {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_nir_struct_name(&param.type_name, &structures) else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            let addresses = target_addresses_by_name
                .get(&sig.name)
                .cloned()
                .unwrap_or_default();
            if addresses.is_empty() {
                call_param_rules.push(NirCallParamRule {
                    callee_address: None,
                    callee_name: sig.name.clone(),
                    arg_index,
                    pointer_alias: param.type_name.clone(),
                    pointee_alias: struct_name.clone(),
                    pointer_size: 8,
                    pointee_sizes: vec![struct_def.size_64 as u32],
                });
            } else {
                for address in addresses {
                    call_param_rules.push(NirCallParamRule {
                        callee_address: Some(address),
                        callee_name: sig.name.clone(),
                        arg_index,
                        pointer_alias: param.type_name.clone(),
                        pointee_alias: struct_name.clone(),
                        pointer_size: 8,
                        pointee_sizes: vec![struct_def.size_64 as u32],
                    });
                }
            }
        }
    }
    call_param_rules
}

fn resolve_nir_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }
    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        let Some(candidate) = normalize_named_type_identity(candidate) else {
            continue;
        };
        if structures.get(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::summarize_preview_callee_effects;
    use crate::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    fn op(seq_num: u32, opcode: PcodeOpcode) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x401000 + seq_num as u64,
            output: None,
            inputs: Vec::new(),
            asm_mnemonic: None,
        }
    }

    fn constant_call_op(seq_num: u32, target: u64) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode: PcodeOpcode::Call,
            address: 0x401000 + seq_num as u64,
            output: None,
            inputs: vec![Varnode::constant(target as i64, 8)],
            asm_mnemonic: None,
        }
    }

    fn test_pcode(ops: Vec<PcodeOp>) -> PcodeFunction {
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x401000,
                successors: Vec::new(),
                ops,
            }],
        }
    }

    #[test]
    fn preview_callee_effect_summary_marks_leaf_return_as_non_exiting() {
        let pcode = test_pcode(vec![op(0, PcodeOpcode::Copy), op(1, PcodeOpcode::Return)]);
        let (summary, detail) = summarize_preview_callee_effects(&pcode);
        assert_eq!(summary.reads_memory, Some(false));
        assert_eq!(summary.writes_memory, Some(false));
        assert_eq!(summary.may_call_unknown, Some(false));
        assert_eq!(summary.may_exit, Some(false));
        assert_eq!(detail.return_count, 1);
    }

    #[test]
    fn preview_callee_effect_summary_marks_store_and_nested_call_as_unsafe() {
        let pcode = test_pcode(vec![
            op(0, PcodeOpcode::Load),
            op(1, PcodeOpcode::Store),
            constant_call_op(2, 0x500000),
            op(3, PcodeOpcode::Return),
        ]);
        let (summary, detail) = summarize_preview_callee_effects(&pcode);
        assert_eq!(summary.reads_memory, Some(true));
        assert_eq!(summary.writes_memory, Some(true));
        assert_eq!(summary.may_call_unknown, Some(true));
        assert_eq!(summary.may_exit, None);
        assert_eq!(detail.store_count, 1);
        assert_eq!(detail.call_count, 1);
        assert_eq!(detail.first_store, Some((0x401001, PcodeOpcode::Store)));
        assert_eq!(
            detail.first_call,
            Some((0x401002, Some(0x500000), PcodeOpcode::Call))
        );
    }
}
