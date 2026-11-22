//! Phase 2 DIE JSON subset — raw matches recorded separately from promoted [`IdentityDetection`].

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use crate::detector::die_engine::{
    DieMatcher, PreparedSignatures, Signature, SignatureDatabase, SignatureRule,
};
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

/// The binary-independent half of the DIE identity lane: the coverage
/// summary, the filtered database prepared for matching, and the unfiltered
/// signatures their raw matches are reported against.
struct PreparedDieIdentity {
    summary: DieCompatSummary,
    /// `None` when no signature has a supported rule.
    filtered: Option<Arc<PreparedSignatures>>,
    originals: Vec<Signature>,
}

impl PreparedDieIdentity {
    fn new(db: SignatureDatabase) -> Self {
        let originals = db.signatures.clone();
        let summary = index_die_compat(&db);
        let filtered = filter_die_database(db);
        let filtered =
            (!filtered.signatures.is_empty()).then(|| Arc::new(PreparedSignatures::new(filtered)));
        Self {
            summary,
            filtered,
            originals,
        }
    }

    /// [`SignatureDatabase::pe_json_only`], prepared once per process. The
    /// initialiser does no rayon work (see [`PreparedSignatures::default_shared`]).
    fn pe_json_shared() -> Option<&'static Self> {
        static PE_JSON: OnceLock<Option<PreparedDieIdentity>> = OnceLock::new();
        PE_JSON
            .get_or_init(|| SignatureDatabase::pe_json_only().map(|db| Self::new(db.clone())))
            .as_ref()
    }

    fn analyze(
        &self,
        binary: &LoadedBinary,
        limits: &IdentityScanLimits,
    ) -> (DieCompatSummary, Vec<DieRawMatch>) {
        let mut summary = self.summary.clone();
        let Some(filtered) = &self.filtered else {
            return (summary, Vec::new());
        };

        let mut matcher = DieMatcher::from_prepared(Arc::clone(filtered)).with_scan_budgets(
            Some(limits.max_string_scan_bytes),
            Some(limits.max_scan_bytes),
        );
        matcher.prepare_direct_signature_strings(binary);

        let raw = collect_die_raw_matches(binary, &matcher, filtered.database(), &self.originals);
        summary.signatures_matched = raw.len();
        (summary, raw)
    }
}

/// Raw DIE subset hits (`pe_signatures.json` only). Promotion happens in [`super::policy`].
pub(super) fn die_compat_identity(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
) -> (Option<DieCompatSummary>, Vec<DieRawMatch>) {
    let Some(prepared) = PreparedDieIdentity::pe_json_shared() else {
        return (None, Vec::new());
    };

    let (summary, raw) = prepared.analyze(binary, limits);
    (Some(summary), raw)
}

/// [`die_compat_identity`] over `db`, prepared from scratch.
#[cfg(test)]
pub(super) fn die_compat_identity_with_db(
    binary: &LoadedBinary,
    limits: &IdentityScanLimits,
    db: SignatureDatabase,
) -> (DieCompatSummary, Vec<DieRawMatch>) {
    PreparedDieIdentity::new(db).analyze(binary, limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn prepared_identity_strings_preserve_complete_legacy_raw_evidence() {
        for format in ["PE", "ELF"] {
            for cap in [0, 2, 64] {
                let bytes = b"prefix needle suffix".to_vec();
                let size = bytes.len() as u64;
                let binary =
                    LoadedBinaryBuilder::new("identity.bin".into(), DataBuffer::Heap(bytes))
                        .format(format)
                        .entry_point(0x1000)
                        .add_section(SectionInfo {
                            name: ".text".into(),
                            virtual_address: 0x1000,
                            virtual_size: size,
                            file_offset: 0,
                            file_size: size,
                            is_executable: true,
                            is_readable: true,
                            is_writable: false,
                        })
                        .build()
                        .unwrap();
                let db = SignatureDatabase {
                    format_version: "fixture".into(),
                    description: "fixture".into(),
                    source: "fixture".into(),
                    signatures: vec![
                        Signature {
                            name: "pe-evidence".into(),
                            sig_type: "compiler".into(),
                            source_format: Some("PE".into()),
                            source_file: Some("fixture/PE/rule.sg".into()),
                            unsupported_rule_count: 1,
                            rules: vec![
                                SignatureRule::StringMatch {
                                    value: "needle".into(),
                                },
                                SignatureRule::SectionName {
                                    name: ".text".into(),
                                },
                                SignatureRule::EpPattern {
                                    arch: None,
                                    pattern: "70".into(),
                                    offset: Some(0),
                                },
                            ],
                        },
                        Signature {
                            name: "elf-evidence".into(),
                            sig_type: "framework".into(),
                            source_format: Some("ELF".into()),
                            source_file: None,
                            unsupported_rule_count: 0,
                            rules: vec![
                                SignatureRule::StringMatch {
                                    value: "needle".into(),
                                },
                                SignatureRule::StringMatch {
                                    value: "needle".into(),
                                },
                                SignatureRule::StringMatch {
                                    value: "missing".into(),
                                },
                            ],
                        },
                        Signature {
                            name: "unprepared-section".into(),
                            sig_type: "tool".into(),
                            source_format: None,
                            source_file: None,
                            unsupported_rule_count: 0,
                            rules: vec![SignatureRule::SectionName {
                                name: ".text".into(),
                            }],
                        },
                    ],
                };
                let limits = IdentityScanLimits {
                    max_string_scan_bytes: cap,
                    ..Default::default()
                };
                // The exact old direct-call path, with no precomputed caches.
                let mut expected_summary = index_die_compat(&db);
                let filtered = filter_die_database(db.clone());
                let legacy = DieMatcher::new(filtered.clone()).with_scan_budgets(
                    Some(limits.max_string_scan_bytes),
                    Some(limits.max_scan_bytes),
                );
                let expected_raw =
                    collect_die_raw_matches(&binary, &legacy, &filtered, &db.signatures);
                expected_summary.signatures_matched = expected_raw.len();
                let actual = die_compat_identity_with_db(&binary, &limits, db);
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    serde_json::to_value((expected_summary, expected_raw)).unwrap()
                );
            }
        }
    }

    /// The identity lane prepared once per process reports, for every loader
    /// fixture, what preparing `pe_signatures.json` from scratch reports.
    #[test]
    fn the_shared_identity_lane_reports_what_a_fresh_one_does() {
        let db = SignatureDatabase::pe_json_only().expect("pe_signatures.json");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .expect("testdata")
            .map(|entry| entry.expect("testdata entry").path())
            .collect();
        paths.sort();
        let (mut loaded, mut matched) = (0, 0);
        for path in paths {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Ok(binary) = LoadedBinary::from_bytes(std::fs::read(&path).unwrap(), name.clone())
            else {
                continue;
            };
            for limits in [
                IdentityScanLimits::default(),
                IdentityScanLimits {
                    max_string_scan_bytes: 64,
                    max_scan_bytes: 64,
                    ..Default::default()
                },
            ] {
                let (summary, raw) = die_compat_identity(&binary, &limits);
                let fresh = die_compat_identity_with_db(&binary, &limits, db.clone());
                assert_eq!(
                    serde_json::to_value((summary.expect("summary"), &raw)).unwrap(),
                    serde_json::to_value(fresh).unwrap(),
                    "{name}"
                );
                matched += raw.len();
            }
            loaded += 1;
        }
        assert!(loaded >= 20, "only {loaded} fixtures loaded");
        assert!(
            matched > 0,
            "the fixtures exercise no DiE identity signature"
        );
    }
}
