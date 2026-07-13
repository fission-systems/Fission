use crate::decode_rust_sleigh_pcode;
use crate::pipeline::rust_sleigh::apply_spec_overrides;
use crate::{
    CallEdgeKind, CallEffectSummarySource, CallTargetProvenance, CallTargetRef,
    NirCallEffectSummary, NirCallParamRule, NirCallPrototypeSummary, NirFunctionHints,
    NirRenderOptions, NirTypeContext, PcodeFunction, PcodeOpcode, RegisterNamer,
    infer_entry_register_param_arity,
};
use fission_analysis_db::SymbolKind;
use fission_core::PATHS;
use fission_core::core::ghidra_no_return::{
    binary_format_to_ghidra_format, ghidra_no_return_index,
};
use fission_core::{normalize_named_type_identity, sanitize_symbol_name};
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_signatures::SIGNATURE_RESOURCES;
use fission_signatures::golang_typeinfo::GoTypeinfoDatabase;
use fission_signatures::win_types::WindowsStructures;
use fission_static::analysis::decomp::facts::FactProvenance;
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn get_well_known_function_hints(name: &str) -> Option<NirFunctionHints> {
    let lower = name.to_ascii_lowercase();
    if lower == "main" || lower == "wmain" || lower == "winmain" || lower == "wwinmain" {
        return None;
    }
    let sigs_iter = SIGNATURE_RESOURCES.api_signatures().ok()?;
    let matched_sig = sigs_iter.into_iter().find(|sig| sig.name == name)?;

    let mut param_names = Vec::new();
    let mut param_type_names = HashMap::new();
    for (index, param) in matched_sig.params.iter().enumerate() {
        param_names.push(param.name.clone());
        param_type_names.insert(index, param.type_name.clone());
    }

    Some(NirFunctionHints {
        param_names,
        param_type_names,
        stack_local_names: HashMap::new(),
        stack_local_type_names: HashMap::new(),
        return_type_name: Some(matched_sig.return_type.clone()),
    })
}

fn get_go_function_hints(name: &str, binary: &LoadedBinary) -> Option<NirFunctionHints> {
    let go_ver = binary.go_version.as_deref()?;
    let typeinfo_dir = PATHS.get_golang_typeinfo_dir()?;
    let goos = GoTypeinfoDatabase::goos_from_format(&binary.format);
    let goarch = GoTypeinfoDatabase::goarch_from_spec(binary.is_64bit, &binary.arch_spec);
    let db = GoTypeinfoDatabase::get_cached(go_ver, goos, goarch, &typeinfo_dir)?;
    let sig = db.get_func(name)?;

    let mut param_names = Vec::new();
    let mut param_type_names = HashMap::new();
    for (index, (pname, ptype)) in sig.params.iter().enumerate() {
        param_names.push(pname.clone());
        param_type_names.insert(index, ptype.clone());
    }
    let return_type_name = sig.results.first().map(|(_, t)| t.clone());

    Some(NirFunctionHints {
        param_names,
        param_type_names,
        stack_local_names: HashMap::new(),
        stack_local_type_names: HashMap::new(),
        return_type_name,
    })
}

pub(crate) fn build_nir_type_context(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> NirTypeContext {
    let mut index = CallTargetIndex::default();
    let mut iat_index = CallTargetIndex::default();
    let program = fact_store.program();

    for function in program
        .functions
        .iter()
        .filter(|function| function.is_import)
    {
        if function.entry == 0 || function.name.is_empty() {
            continue;
        }
        iat_index.add(function.entry, &function.name, CandidateClass::Import);
    }

    for symbol in program
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Import)
    {
        if symbol.address == 0 || symbol.name.is_empty() {
            continue;
        }
        iat_index.add(symbol.address, &symbol.name, CandidateClass::Import);
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

    for function in &program.functions {
        if function.name.is_empty() {
            continue;
        }
        if function.is_import {
            continue;
        }
        let class = if function.is_export && function.is_thunk {
            CandidateClass::ExportThunk
        } else if function.is_export {
            CandidateClass::Export
        } else {
            CandidateClass::Direct
        };
        index.add(function.entry, &function.name, class);
        if function.is_export
            && function.is_thunk
            && let Some(thunk_target) = function.thunk_target
            && thunk_target != 0
        {
            index.add(
                thunk_target,
                &function.name,
                CandidateClass::ExportThunkTarget,
            );
        }
    }

    for symbol in program
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Data)
    {
        if symbol.name.is_empty() {
            continue;
        }
        index.add(symbol.address, &symbol.name, CandidateClass::Global);
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

    let mut function_hints = build_nir_function_hints(fact_store, address);
    if function_hints.is_none() {
        let name = all_target_refs
            .get(&address)
            .map(|tr| tr.symbol.as_str())
            .unwrap_or("");
        if let Some(well_known) = get_well_known_function_hints(name) {
            function_hints = Some(well_known);
        } else if let Some(go_hints) = get_go_function_hints(name, binary) {
            function_hints = Some(go_hints);
        }
    }

    NirTypeContext {
        call_targets,
        call_target_refs: call_target_refs.clone(),
        iat_target_refs: iat_target_refs.clone(),
        ambiguous_call_targets: resolved_index.ambiguous_call_targets,
        call_effect_summaries: build_nir_call_effect_summaries(&all_target_refs, binary),
        call_prototype_summaries: HashMap::new(),
        call_param_rules: build_nir_call_param_rules(&all_target_refs),
        function_hints,
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
    binary: &LoadedBinary,
) -> HashMap<String, NirCallEffectSummary> {
    let ghidra_format = binary_format_to_ghidra_format(&binary.format);
    let compiler_key = ghidra_no_return_compiler_key(binary);
    let no_return_idx = ghidra_no_return_index();

    let mut result: HashMap<String, NirCallEffectSummary> = HashMap::new();

    for (&address, target_ref) in call_target_refs {
        let library_name: Option<&str> = binary
            .function_at_exact(address)
            .and_then(|f| f.external_library.as_deref());

        let may_exit = ghidra_format.and_then(|fmt| {
            if no_return_idx.is_no_return(fmt, compiler_key, library_name, &target_ref.symbol) {
                Some(true)
            } else {
                None
            }
        });

        let source = if may_exit.is_some() {
            Some(CallEffectSummarySource::GhidraNoReturnData)
        } else {
            Some(CallEffectSummarySource::CallTargetRef)
        };

        let entry = result
            .entry(target_ref.symbol.clone())
            .or_insert(NirCallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                may_call_unknown: None,
                may_exit,
                source,
            });
        // Upgrade to may_exit=true if a later address for the same symbol name provides evidence.
        if entry.may_exit.is_none() && may_exit.is_some() {
            entry.may_exit = may_exit;
            entry.source = source;
        }
    }

    result
}

fn ghidra_no_return_compiler_key(binary: &LoadedBinary) -> Option<&'static str> {
    let lang = binary
        .identity_report
        .as_ref()?
        .summary
        .likely_language
        .as_deref()?;
    match lang.to_ascii_lowercase().as_str() {
        "go" | "golang" => Some("golang"),
        "rust" => Some("rustc"),
        _ => None,
    }
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
    let mut options = NirRenderOptions::from_loaded_binary(binary);
    apply_spec_overrides(binary, &mut options);
    let register_namer = RegisterNamer::from_options(&options);
    let prototype = infer_entry_register_param_arity(&pcode, &register_namer).map(|arity| {
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
    let debug_hints = fact_store
        .preferred_debug_function(address)
        .and_then(nir_hints_from_debug_function);
    merge_nir_function_hints(debug_hints, fact_store.structuring_hints(address))
}

fn nir_hints_from_debug_function(
    debug: &fission_loader::loader::types::DwarfFunctionInfo,
) -> Option<NirFunctionHints> {
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

fn merge_nir_function_hints(
    debug: Option<NirFunctionHints>,
    structural: Option<&NirFunctionHints>,
) -> Option<NirFunctionHints> {
    let mut merged = debug.unwrap_or_default();
    let Some(structural) = structural else {
        return (!nir_function_hints_are_empty(&merged)).then_some(merged);
    };

    if merged.param_names.len() < structural.param_names.len() {
        merged
            .param_names
            .resize(structural.param_names.len(), String::new());
    }
    for (index, name) in structural.param_names.iter().enumerate() {
        if merged.param_names[index].is_empty() && !name.is_empty() {
            merged.param_names[index] = name.clone();
        }
    }
    for (index, type_name) in &structural.param_type_names {
        merged
            .param_type_names
            .entry(*index)
            .or_insert_with(|| type_name.clone());
    }
    for (offset, name) in &structural.stack_local_names {
        merged
            .stack_local_names
            .entry(*offset)
            .or_insert_with(|| name.clone());
    }
    for (offset, type_name) in &structural.stack_local_type_names {
        merged
            .stack_local_type_names
            .entry(*offset)
            .or_insert_with(|| type_name.clone());
    }
    if merged.return_type_name.is_none() {
        merged
            .return_type_name
            .clone_from(&structural.return_type_name);
    }

    (!nir_function_hints_are_empty(&merged)).then_some(merged)
}

fn nir_function_hints_are_empty(hints: &NirFunctionHints) -> bool {
    hints.param_names.iter().all(String::is_empty)
        && hints.param_type_names.is_empty()
        && hints.stack_local_names.is_empty()
        && hints.stack_local_type_names.is_empty()
        && hints.return_type_name.is_none()
}

pub(crate) fn sanitize_nir_symbol_name(name: &str) -> String {
    sanitize_symbol_name(name)
}

fn build_nir_call_param_rules(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> Vec<NirCallParamRule> {
    let mut call_param_rules = Vec::new();
    let Ok(structures) = WindowsStructures::try_new() else {
        return call_param_rules;
    };
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
    use super::{
        build_nir_call_param_rules, merge_nir_function_hints, resolve_nir_struct_name,
        summarize_preview_callee_effects,
    };
    use crate::{
        CallEdgeKind, CallTargetProvenance, CallTargetRef, PcodeBasicBlock, PcodeFunction, PcodeOp,
        PcodeOpcode, Varnode,
    };
    use fission_signatures::win_types::WindowsStructures;
    use std::collections::HashMap;

    #[test]
    fn debug_hints_keep_precedence_over_structuring_overlay() {
        let debug = crate::NirFunctionHints {
            param_names: vec!["debug_name".into()],
            param_type_names: HashMap::from([(0, "int".into())]),
            return_type_name: Some("int".into()),
            ..Default::default()
        };
        let structural = crate::NirFunctionHints {
            param_names: vec!["structural_name".into(), "second".into()],
            param_type_names: HashMap::from([(0, "uint32_t".into()), (1, "char *".into())]),
            return_type_name: Some("uint32_t".into()),
            ..Default::default()
        };

        let merged = merge_nir_function_hints(Some(debug), Some(&structural)).unwrap();
        assert_eq!(merged.param_names, ["debug_name", "second"]);
        assert_eq!(merged.param_type_names[&0], "int");
        assert_eq!(merged.param_type_names[&1], "char *");
        assert_eq!(merged.return_type_name.as_deref(), Some("int"));
    }

    // -------------------------------------------------------------------------
    // GDT pattern matching: resolve_nir_struct_name
    // -------------------------------------------------------------------------

    #[test]
    fn gdt_pattern_match_lp_prefix_resolves_to_struct() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PSECURITY_DESCRIPTOR", &ws),
            Some("SECURITY_DESCRIPTOR".to_string()),
            "PSECURITY_DESCRIPTOR -> SECURITY_DESCRIPTOR via P-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_p_prefix_resolves_sid() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PSID", &ws),
            Some("SID".to_string()),
            "PSID -> SID via P-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_lp_prefix_resolves_critical_section() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("LPCRITICAL_SECTION", &ws),
            Some("CRITICAL_SECTION".to_string()),
            "LPCRITICAL_SECTION -> CRITICAL_SECTION via LP-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_pointer_star_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("SECURITY_DESCRIPTOR*", &ws),
            None,
            "pointer-star types must be rejected"
        );
    }

    #[test]
    fn gdt_pattern_match_bare_type_no_prefix_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("UINT", &ws),
            None,
            "bare scalar type without LP/P prefix must not match"
        );
    }

    #[test]
    fn gdt_pattern_match_p_prefix_unknown_struct_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PVOID", &ws),
            None,
            "PVOID -> VOID is not a known struct"
        );
    }

    // -------------------------------------------------------------------------
    // GDT pattern matching: build_nir_call_param_rules (end-to-end)
    // -------------------------------------------------------------------------

    fn make_call_target_refs(entries: &[(u64, &str)]) -> HashMap<u64, CallTargetRef> {
        entries
            .iter()
            .map(|&(addr, name)| {
                (
                    addr,
                    CallTargetRef {
                        address: Some(addr),
                        symbol: name.to_string(),
                        provenance: CallTargetProvenance::Import,
                        edge_kind: CallEdgeKind::Import,
                        confidence: 255,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn gdt_call_param_rules_generated_for_known_win32_api() {
        let refs = make_call_target_refs(&[(0x1000, "AccessCheckAndAuditAlarmA")]);
        let rules = build_nir_call_param_rules(&refs);
        assert!(
            !rules.is_empty(),
            "expected at least one NirCallParamRule for AccessCheckAndAuditAlarmA \
             (has PSECURITY_DESCRIPTOR param)"
        );
        let sd_rule = rules.iter().find(|r| {
            r.callee_name == "AccessCheckAndAuditAlarmA" && r.pointee_alias == "SECURITY_DESCRIPTOR"
        });
        assert!(
            sd_rule.is_some(),
            "expected a rule for SECURITY_DESCRIPTOR param of AccessCheckAndAuditAlarmA"
        );
    }

    #[test]
    fn gdt_call_param_rules_empty_for_unknown_function() {
        let refs = make_call_target_refs(&[(0x2000, "NonExistentFunctionXYZ")]);
        let rules = build_nir_call_param_rules(&refs);
        let matched: Vec<_> = rules
            .iter()
            .filter(|r| r.callee_name == "NonExistentFunctionXYZ")
            .collect();
        assert!(
            matched.is_empty(),
            "unknown function must produce no param rules"
        );
    }

    #[test]
    fn gdt_call_param_rules_no_address_when_not_in_refs() {
        let refs = HashMap::new();
        let rules = build_nir_call_param_rules(&refs);
        let no_addr_rules: Vec<_> = rules
            .iter()
            .filter(|r| r.callee_address.is_none())
            .collect();
        assert!(
            !no_addr_rules.is_empty(),
            "rules without a resolved address must still be emitted for signature-only coverage"
        );
    }

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
