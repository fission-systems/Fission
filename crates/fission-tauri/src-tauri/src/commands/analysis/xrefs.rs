//! Cross-reference analysis — callers and callees of a function.
//!
//! Uses [`fission_static::analysis::XrefDatabase`] built from the same pipeline as static analysis
//! (executable sections + Sleigh operand refs), cached lazily on [`crate::state::InnerState`].

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::{MAX_XREF_DECODE, MAX_XREF_INCOMING, MAX_XREF_OUTGOING};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{Xref, XrefDatabase};
use std::collections::HashMap;
use tauri::State;

fn enclosing_function_display_name(
    binary: &LoadedBinary,
    renamed: &HashMap<u64, String>,
    insn_addr: u64,
) -> Option<String> {
    binary
        .functions
        .iter()
        .find(|f| insn_addr >= f.address && insn_addr < f.address.saturating_add(f.size.max(1)))
        .map(|f| {
            renamed
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone())
        })
}

fn entry_function_display_name(
    binary: &LoadedBinary,
    renamed: &HashMap<u64, String>,
    entry_addr: u64,
) -> Option<String> {
    binary
        .functions
        .iter()
        .find(|f| f.address == entry_addr)
        .map(|f| {
            renamed
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone())
        })
}

fn xref_to_dto(binary: &LoadedBinary, renamed: &HashMap<u64, String>, xref: &Xref) -> XrefDto {
    XrefDto {
        from_address: format!("0x{:x}", xref.from_addr),
        to_address: format!("0x{:x}", xref.to_addr),
        xref_type: xref.flow_tag().to_string(),
        from_function: enclosing_function_display_name(binary, renamed, xref.from_addr),
        to_function: entry_function_display_name(binary, renamed, xref.to_addr),
        operand_index: Some(xref.operand_index),
        sleigh_kind: xref.sleigh_kind.map(|k| format!("{k:?}")),
    }
}

/// Get cross-references for an address (incoming refs-to plus outgoing refs from the selected function body).
#[tauri::command]
pub async fn get_xrefs(address: u64, state: State<'_, AppState>) -> CmdResult<Vec<XrefDto>> {
    let mut inner = state.inner.lock().await;
    let binary_arc = inner
        .loaded_binary
        .clone()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;
    let binary = binary_arc.as_ref();
    let renamed = inner.renamed_functions.clone();

    if inner.xref_database.is_none() {
        inner.xref_database = Some(XrefDatabase::build_from_binary(binary));
    }
    let db = inner
        .xref_database
        .as_ref()
        .ok_or_else(|| CmdError::other("internal: xref database missing after build"))?;

    let mut results = Vec::new();

    for xref in db.get_refs_to(address).iter().take(MAX_XREF_INCOMING) {
        results.push(xref_to_dto(binary, &renamed, xref));
    }

    if let Some(func) = binary.functions.iter().find(|f| f.address == address) {
        let window = func.size.max(1).min(MAX_XREF_DECODE as u64);
        let hi = address.saturating_add(window);

        let mut outgoing: Vec<&Xref> = db
            .iter()
            .filter(|x| x.from_addr >= address && x.from_addr < hi)
            .collect();
        outgoing.sort_by_key(|x| x.from_addr);

        let mut outgoing_rows = 0usize;
        for xref in outgoing {
            if outgoing_rows >= MAX_XREF_OUTGOING {
                break;
            }
            results.push(xref_to_dto(binary, &renamed, xref));
            outgoing_rows += 1;
        }
    }

    Ok(results)
}
