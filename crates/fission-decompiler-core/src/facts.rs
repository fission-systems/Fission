use crate::decode_rust_sleigh_pcode;
use fission_core::{normalize_named_type_identity, sanitize_symbol_name};
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_pcode::{
    CallEdgeKind, CallEffectSummarySource, CallTargetProvenance, CallTargetRef,
    NirCallEffectSummary, NirCallParamRule, NirFunctionHints, NirTypeContext, PcodeFunction,
    PcodeOpcode,
};
use fission_signatures::WIN_API_DB;
use fission_signatures::win_types::WindowsStructures;
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::{BTreeSet, HashMap};

pub(crate) fn build_nir_type_context(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> NirTypeContext {
    let mut call_targets = HashMap::new();
    let mut call_target_refs = HashMap::new();

    for (resolved_address, fact) in fact_store.iter_resolved_name_facts() {
        if resolved_address == 0 || fact.name.is_empty() {
            continue;
        }
        let sanitized = sanitize_nir_symbol_name(&fact.name);
        call_targets.insert(resolved_address, sanitized.clone());
        call_target_refs.insert(
            resolved_address,
            CallTargetRef {
                address: Some(resolved_address),
                symbol: sanitized,
                provenance: CallTargetProvenance::Fact,
                edge_kind: CallEdgeKind::Reference,
                confidence: 255,
            },
        );
    }

    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        let sanitized = sanitize_nir_symbol_name(&func.name);
        call_targets
            .entry(func.address)
            .or_insert_with(|| sanitized.clone());
        call_target_refs
            .entry(func.address)
            .or_insert(CallTargetRef {
                address: Some(func.address),
                symbol: sanitized,
                provenance: CallTargetProvenance::Direct,
                edge_kind: CallEdgeKind::Direct,
                confidence: 224,
            });
    }

    for (resolved_address, name) in &binary.inner().iat_symbols {
        if *resolved_address == 0 || name.is_empty() {
            continue;
        }
        let sanitized = sanitize_nir_symbol_name(name);
        call_targets
            .entry(*resolved_address)
            .or_insert_with(|| sanitized.clone());
        call_target_refs
            .entry(*resolved_address)
            .or_insert(CallTargetRef {
                address: Some(*resolved_address),
                symbol: sanitized,
                provenance: CallTargetProvenance::Import,
                edge_kind: CallEdgeKind::Import,
                confidence: 255,
            });
    }

    for (resolved_address, name) in &binary.inner().global_symbols {
        if *resolved_address == 0 || name.is_empty() {
            continue;
        }
        let sanitized = sanitize_nir_symbol_name(name);
        call_targets
            .entry(*resolved_address)
            .or_insert_with(|| sanitized.clone());
        call_target_refs
            .entry(*resolved_address)
            .or_insert(CallTargetRef {
                address: Some(*resolved_address),
                symbol: sanitized,
                provenance: CallTargetProvenance::Global,
                edge_kind: CallEdgeKind::Reference,
                confidence: 192,
            });
    }

    NirTypeContext {
        call_targets,
        call_target_refs: call_target_refs.clone(),
        call_effect_summaries: build_nir_call_effect_summaries(&call_target_refs),
        call_param_rules: build_nir_call_param_rules(&call_target_refs),
        function_hints: build_nir_function_hints(fact_store, address),
    }
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
        let Some(summary) =
            build_preview_callee_effect_summary(binary, target_addr, &target_ref.symbol)
        else {
            continue;
        };
        type_context
            .call_effect_summaries
            .insert(target_ref.symbol.clone(), summary);
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
            if !target.is_constant {
                continue;
            }
            callees.insert(target.offset);
        }
    }
    callees
}

fn build_preview_callee_effect_summary(
    binary: &LoadedBinary,
    target_addr: u64,
    target_name: &str,
) -> Option<NirCallEffectSummary> {
    let function = binary.function_at_exact(target_addr)?;
    if function.is_import {
        return None;
    }
    let max_bytes = direct_callee_max_bytes(binary, target_addr)?;
    let instruction_limit = direct_callee_instruction_limit(max_bytes);
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
    trace_preview_callee_effect_detail(target_name, target_addr, &detail);
    Some(summary)
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
    first_store: Option<(u64, PcodeOpcode)>,
    first_call: Option<(u64, Option<u64>, PcodeOpcode)>,
    first_callother: Option<(u64, PcodeOpcode)>,
}

fn summarize_preview_callee_effects(
    pcode: &PcodeFunction,
) -> (NirCallEffectSummary, PreviewCalleeEffectDetail) {
    let mut reads_memory = Some(false);
    let mut writes_memory = Some(false);
    let mut may_call_unknown = Some(false);
    let mut may_exit = None;
    let mut saw_return = false;
    let mut detail = PreviewCalleeEffectDetail::default();

    for block in &pcode.blocks {
        for op in &block.ops {
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
                        op.inputs.first().and_then(|input| input.is_constant.then_some(input.offset)),
                        op.opcode,
                    ));
                }
                PcodeOpcode::CallInd => {
                    may_call_unknown = Some(true);
                    detail.callind_count += 1;
                    detail.first_call.get_or_insert((op.address, None, op.opcode));
                }
                PcodeOpcode::CallOther => {
                    may_call_unknown = Some(true);
                    may_exit = Some(true);
                    detail.callother_count += 1;
                    detail.first_callother.get_or_insert((op.address, op.opcode));
                }
                PcodeOpcode::Return => {
                    saw_return = true;
                    detail.return_count += 1;
                }
                _ => {}
            }
        }
    }

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
    detail: &PreviewCalleeEffectDetail,
) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_none() {
        return;
    }
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
    if let Some((address, opcode)) = detail.first_store {
        eprintln!(
            "[GT-TRACE] callee-effect-first-store target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
    }
    if let Some((address, call_target, opcode)) = detail.first_call {
        eprintln!(
            "[GT-TRACE] callee-effect-first-call target={} addr=0x{:x} call_target={:?} op={:?}",
            target_name, address, call_target, opcode
        );
    }
    if let Some((address, opcode)) = detail.first_callother {
        eprintln!(
            "[GT-TRACE] callee-effect-first-callother target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
    }
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
    for sig in WIN_API_DB.iter() {
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
    use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

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
