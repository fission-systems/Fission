//! Additive observation fields for `debug_decomp` JSON (schema_version 1).

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::{
    build_external_symbol_index, build_function_provenance_index, build_xref_index,
    win_api_prototype_hint_json,
};

pub(crate) fn attach_static_analysis_facts(
    root: &mut serde_json::Value,
    binary: &LoadedBinary,
    func: &FunctionInfo,
) {
    let ext_idx = build_external_symbol_index(binary);
    let prov_idx = build_function_provenance_index(binary, Some(&ext_idx));
    let xref_summary = build_xref_index(binary, false).summary();

    if let Some(rec) = prov_idx.records.get(&func.address) {
        if let Ok(v) = serde_json::to_value(rec) {
            root["function_provenance"] = v;
        }
    }
    if let Some(ck) = ext_idx.canonical_key_for_va(func.address) {
        root["external_symbol_canonical"] = serde_json::Value::String(ck.to_string());
    }
    if let Ok(v) = serde_json::to_value(&xref_summary) {
        root["xref_summary_loader_only"] = v;
    }
    if let Some(hint) = win_api_prototype_hint_json(&func.name) {
        root["prototype_hint_win_api"] = hint;
    }
}
