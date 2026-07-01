//! Phase 2 DIE JSON subset — raw matches recorded separately from promoted [`IdentityDetection`].

use std::collections::BTreeMap;

use crate::detector::die_engine::{DieMatcher, Signature, SignatureDatabase, SignatureRule};
use crate::loader::LoadedBinary;

use super::model::{DieCompatSummary, DieRawMatch, IdentityScanLimits};

#[must_use]
fn die_rule_identity_phase2_supported(rule: &SignatureRule) -> bool {
    !matches!(
        rule,
        SignatureRule::EpPattern { .. }
            | SignatureRule::FilePattern { .. }
            | SignatureRule::OverlayPattern { .. }
    )
}

#[must_use]
fn die_rule_primitive_key(rule: &SignatureRule) -> &'static str {
    match rule {
        SignatureRule::EpPattern { .. } => "ep_pattern",
        SignatureRule::FilePattern { .. } => "file_pattern",
        SignatureRule::OverlayPattern { .. } => "overlay_pattern",
        SignatureRule::SectionName { .. } => "section_name",
        SignatureRule::StringMatch { .. } => "string_match",
        SignatureRule::OverlayPresent { .. } => "overlay_present",
        SignatureRule::SectionCount { .. } => "section_count",
        SignatureRule::SectionNumeric { .. } => "section_numeric",
        SignatureRule::SectionEntropy { .. } => "section_entropy",
        SignatureRule::OverlayEntropy { .. } => "overlay_entropy",
        SignatureRule::Import { .. } => "import",
        SignatureRule::RichHeader { .. } => "rich_header",
    }
}

fn index_die_compat(db: &SignatureDatabase) -> DieCompatSummary {
    let mut unsupported_primitives = BTreeMap::new();
    let mut rules_seen = 0usize;
    let mut rules_supported = 0usize;
    let mut rules_skipped = 0usize;

    for sig in &db.signatures {
        for rule in &sig.rules {
            rules_seen += 1;
            if die_rule_identity_phase2_supported(rule) {
                rules_supported += 1;
            } else {
                rules_skipped += 1;
                let key = die_rule_primitive_key(rule).to_string();
                *unsupported_primitives.entry(key).or_insert(0) += 1;
            }
        }
    }

    let signatures_seen = db.signatures.len();
    let signatures_with_supported_rules = db
        .signatures
        .iter()
        .filter(|s| s.rules.iter().any(die_rule_identity_phase2_supported))
        .count();

    DieCompatSummary {
        rules_seen,
        rules_supported,
        rules_skipped,
        signatures_seen,
        signatures_with_supported_rules,
        signatures_matched: 0,
        unsupported_primitives,
    }
}

fn filter_die_database(mut db: SignatureDatabase) -> SignatureDatabase {
    db.signatures = db
        .signatures
        .into_iter()
        .filter_map(|mut sig| {
            let rules: Vec<_> = sig
                .rules
                .into_iter()
                .filter(die_rule_identity_phase2_supported)
                .collect();
            if rules.is_empty() {
                return None;
            }
            sig.rules = rules;
            sig.unsupported_rule_count = 0;
            Some(sig)
        })
        .collect();
    db
}

fn collect_die_raw_matches(
    binary: &LoadedBinary,
    matcher: &DieMatcher,
    filtered: &SignatureDatabase,
    originals: &[Signature],
) -> Vec<DieRawMatch> {
    let mut raw = Vec::new();
    for sig in &filtered.signatures {
        let Some(det) = matcher.match_signature(binary, sig) else {
            continue;
        };
        let orig = originals
            .iter()
            .find(|s| s.name == sig.name && s.sig_type == sig.sig_type)
            .unwrap_or(sig);

        let matched_primitive_labels: Vec<String> = sig
            .rules
            .iter()
            .filter(|r| matcher.eval_signature_rule(binary, r))
            .map(|r| die_rule_primitive_key(r).to_string())
            .collect();

        let matched_supported = matched_primitive_labels.len();
        let unsupported_body = orig
            .rules
            .iter()
            .filter(|r| !die_rule_identity_phase2_supported(r))
            .count();

        let total_primitive_slots = orig
            .rules
            .len()
            .saturating_add(orig.unsupported_rule_count)
            .max(1);

        raw.push(DieRawMatch {
            rule_id: format!("die:{}::{}", sig.sig_type, sig.name),
            rule_name: sig.name.clone(),
            category: sig.sig_type.clone(),
            matched_primitives: matched_primitive_labels,
            unsupported_primitives_ignored: unsupported_body
                .saturating_add(orig.unsupported_rule_count),
            matched_primitive_count: matched_supported,
            total_primitive_slots,
            raw_score_bonus: 0,
            details: det.details.clone().unwrap_or_else(|| det.display()),
        });
    }
    raw
}

/// Raw DIE subset hits (`pe_signatures.json` only). Promotion happens in [`super::policy`].
pub(super) fn die_compat_identity(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
) -> (Option<DieCompatSummary>, Vec<DieRawMatch>) {
    let Some(db) = SignatureDatabase::load_pe_json_only() else {
        return (None, Vec::new());
    };

    let (summary, raw) = die_compat_identity_with_db(binary, limits, db);
    (Some(summary), raw)
}

pub(super) fn die_compat_identity_with_db(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
    db: SignatureDatabase,
) -> (DieCompatSummary, Vec<DieRawMatch>) {
    let originals = db.signatures.clone();
    let mut summary = index_die_compat(&db);
    let filtered = filter_die_database(db);
    if filtered.signatures.is_empty() {
        return (summary, Vec::new());
    }

    let matcher = DieMatcher::new(filtered.clone()).with_scan_budgets(
        Some(limits.max_string_scan_bytes),
        Some(limits.max_scan_bytes),
    );

    let raw = collect_die_raw_matches(binary, &matcher, &filtered, &originals);
    summary.signatures_matched = raw.len();
    (summary, raw)
}
