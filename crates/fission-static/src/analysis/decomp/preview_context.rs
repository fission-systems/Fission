use crate::analysis::decomp::FactStore;
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_pcode::{PreviewCallParamRule, PreviewFunctionHints, PreviewTypeContext};
use fission_signatures::WIN_API_DB;
use fission_signatures::win_types::WindowsStructures;
use std::collections::HashMap;

pub(crate) fn build_preview_type_context(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> PreviewTypeContext {
    let mut call_targets = HashMap::new();

    for (resolved_address, fact) in fact_store.iter_resolved_name_facts() {
        if resolved_address == 0 || fact.name.is_empty() {
            continue;
        }
        call_targets.insert(resolved_address, sanitize_preview_symbol_name(&fact.name));
    }

    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        call_targets
            .entry(func.address)
            .or_insert_with(|| sanitize_preview_symbol_name(&func.name));
    }

    for (resolved_address, name) in &binary.inner().iat_symbols {
        if *resolved_address == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*resolved_address)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }

    for (resolved_address, name) in &binary.inner().global_symbols {
        if *resolved_address == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*resolved_address)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }

    PreviewTypeContext {
        call_targets,
        call_param_rules: build_preview_call_param_rules(),
        function_hints: build_preview_function_hints(fact_store, address),
    }
}

fn build_preview_function_hints(
    fact_store: &FactStore,
    address: u64,
) -> Option<PreviewFunctionHints> {
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
        Some(PreviewFunctionHints {
            param_names,
            param_type_names,
            stack_local_names,
            stack_local_type_names,
            return_type_name,
        })
    }
}

fn sanitize_preview_symbol_name(name: &str) -> String {
    let mut sanitized = name.trim().to_string();
    if let Some((_, tail)) = sanitized.rsplit_once('!') {
        sanitized = tail.trim().to_string();
    }
    if let Some(stripped) = sanitized.strip_prefix("__imp_") {
        sanitized = stripped.trim().to_string();
    }
    for suffix in [" [import]", " [export]"] {
        if let Some(stripped) = sanitized.strip_suffix(suffix) {
            sanitized = stripped.trim_end().to_string();
        }
    }
    sanitized
}

fn build_preview_call_param_rules() -> Vec<PreviewCallParamRule> {
    let structures = WindowsStructures::new();
    let mut call_param_rules = Vec::new();
    for sig in WIN_API_DB.iter() {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_preview_struct_name(&param.type_name, &structures)
            else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            call_param_rules.push(PreviewCallParamRule {
                callee_name: sig.name.clone(),
                arg_index,
                pointer_alias: param.type_name.clone(),
                pointee_alias: struct_name,
                pointer_size: 8,
                pointee_sizes: vec![struct_def.size_64 as u32],
            });
        }
    }
    call_param_rules
}

fn resolve_preview_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }
    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        if structures.get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }
    None
}
