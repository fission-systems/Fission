//! MSVC CRT JSON patterns (`fission-signatures`) + WinAPI catalog coverage (informational).

use fission_signatures::{ApiTypeDatabase, SIGNATURE_DB};

use crate::loader::LoadedBinary;

use super::evidence::EvidenceBudget;
use super::model::{
    EvidenceLocation, IdentityDetection, IdentityKind, IdentitySource, WinApiCatalogSummary,
};
use super::pe::is_pe_format;
use super::scoring::{distinct_evidence_sources, gate_high_for_kind};

const ENTRY_PATTERN_WINDOW: usize = 512;

#[must_use]
pub(super) fn signature_pattern_identity(
    binary: &LoadedBinary,
    evidence_budget: &mut EvidenceBudget,
) -> Vec<IdentityDetection> {
    if !is_pe_format(&binary.format) {
        return Vec::new();
    }
    let Some(slice) = entry_window_bytes(binary, ENTRY_PATTERN_WINDOW) else {
        return Vec::new();
    };
    let Some(hit) = SIGNATURE_DB.identify(slice) else {
        return Vec::new();
    };

    let mut evidence = Vec::new();
    evidence_budget.push_simple(
        &mut evidence,
        IdentitySource::SignaturePattern,
        EvidenceLocation::VirtualAddress(binary.entry_point),
        "CRT/runtime pattern matched near entry (bounded window)",
        hit.name.clone(),
    );

    let distinct = distinct_evidence_sources(&evidence);
    let score = 3_u32;
    let confidence = gate_high_for_kind(IdentityKind::Runtime, score, distinct);

    vec![IdentityDetection {
        kind: IdentityKind::Runtime,
        name: hit.name.clone(),
        version: None,
        confidence,
        source: IdentitySource::SignaturePattern,
        score,
        evidence,
        negative_evidence: Vec::new(),
    }]
}

fn entry_window_bytes(binary: &LoadedBinary, window: usize) -> Option<&[u8]> {
    let data = binary.data.as_slice();
    let ep_va = binary.entry_point;
    binary
        .sections
        .iter()
        .find(|s| {
            ep_va >= s.virtual_address && ep_va < s.virtual_address.saturating_add(s.virtual_size)
        })
        .and_then(|s| {
            let offset_in_section = ep_va.checked_sub(s.virtual_address)?;
            let file_off = usize::try_from(s.file_offset)
                .ok()?
                .checked_add(usize::try_from(offset_in_section).unwrap_or(usize::MAX))?;
            let end = file_off.checked_add(window)?.min(data.len());
            data.get(file_off..end)
        })
}

#[must_use]
pub(super) fn winapi_catalog_summary(binary: &LoadedBinary) -> Option<WinApiCatalogSummary> {
    let db = ApiTypeDatabase::from_utils_signatures().ok()?;
    let considered = binary.iat_symbols.len();
    let mut symbols_in_catalog = 0usize;
    for sym in binary.iat_symbols.values() {
        if symbol_in_winapi_catalog(&db, sym) {
            symbols_in_catalog += 1;
        }
    }
    Some(WinApiCatalogSummary {
        symbols_considered: considered,
        symbols_in_catalog,
        symbols_not_in_catalog: considered.saturating_sub(symbols_in_catalog),
    })
}

fn symbol_in_winapi_catalog(db: &ApiTypeDatabase, raw: &str) -> bool {
    if db.get(raw).is_some() {
        return true;
    }
    let unstripped = raw.strip_prefix('_').unwrap_or(raw);
    if db.get(unstripped).is_some() {
        return true;
    }
    let dm = crate::loader::demangle::demangle(raw);
    let trimmed = dm.trim();
    if db.get(trimmed).is_some() {
        return true;
    }
    for token in trimmed.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token.len() >= 4 && db.get(token).is_some() {
            return true;
        }
    }
    false
}
