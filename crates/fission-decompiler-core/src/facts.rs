use fission_core::{normalize_named_type_identity, sanitize_symbol_name};
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_pcode::{
    CallEdgeKind, CallEffectSummarySource, CallTargetProvenance, CallTargetRef,
    NirCallEffectSummary, NirCallParamRule, NirFunctionHints, NirTypeContext,
};
use fission_signatures::WIN_API_DB;
use fission_signatures::win_types::WindowsStructures;
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::HashMap;

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
