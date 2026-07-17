use super::*;

pub(crate) fn recovered_switch_case_values(
    targets: &[u64],
    default_target: Option<u64>,
    min_val: i64,
    proof: Option<&DispatcherProofUnit>,
) -> (Vec<(i64, u64)>, bool) {
    if let Some(proof) = proof
        && proof_supports_direct_emit(proof)
    {
        let recovered = proof
            .recovered_cases
            .iter()
            .copied()
            .filter(|(_, target)| Some(*target) != default_target)
            .collect::<Vec<_>>();
        if !recovered.is_empty() {
            return (recovered, true);
        }
    }

    (
        targets
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(ordinal, target)| {
                (Some(target) != default_target).then_some((min_val + ordinal as i64, target))
            })
            .collect(),
        false,
    )
}

pub(crate) fn proof_supports_direct_emit(proof: &DispatcherProofUnit) -> bool {
    crate::midend::structuring::EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready
        && proof.recovered_cases.len() >= proof.selector_cardinality
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_legality() -> DispatcherLegality {
        DispatcherLegality {
            follow_block: Some(0x1300),
            postdom_ok: true,
            side_effect_free_selector: true,
            ordinal_domain_complete: true,
            shared_tail_conflict: false,
            valid: true,
        }
    }

    fn proof_with_cases(
        recovered_cases: Vec<(i64, u64)>,
        selector_cardinality: usize,
        proof_complete: bool,
        failure_family: Option<ProofFailureFamily>,
    ) -> DispatcherProofUnit {
        DispatcherProofUnit {
            selector_expr: "selector".to_string(),
            rendered_selector_expr: Some("selector".to_string()),
            candidate_targets: recovered_cases.iter().map(|(_, target)| *target).collect(),
            recovered_cases,
            selector_cardinality,
            target_cardinality: 2,
            case_map_source: DispatcherCaseMapSource::Merged,
            default_target: Some(0x1300),
            guard_set: vec!["ordinal_domain_complete".to_string()],
            follow_block: Some(0x1300),
            normalization: None,
            legality_witness: Some(complete_legality()),
            proof_scope: DispatcherProofScope::OuterDispatch,
            proof_complete,
            failure_family,
        }
    }

    #[test]
    fn proof_supports_direct_emit_allows_many_to_one_case_map() {
        let proof = proof_with_cases(vec![(0, 0x1100), (1, 0x1100), (2, 0x1200)], 3, true, None);
        assert!(proof_supports_direct_emit(&proof));
    }

    #[test]
    fn recovered_switch_case_values_ignore_incomplete_proof_payload() {
        let proof = proof_with_cases(
            vec![(0, 0x1100), (1, 0x1200)],
            2,
            false,
            Some(ProofFailureFamily::MissingOrdinalCoverage),
        );
        let (cases, used_proof_payload) =
            recovered_switch_case_values(&[0x1100, 0x1200], Some(0x1300), 7, Some(&proof));
        assert!(!used_proof_payload);
        assert_eq!(cases, vec![(7, 0x1100), (8, 0x1200)]);
    }

    #[test]
    fn emit_ready_decision_requires_complete_proof() {
        let proof = proof_with_cases(
            vec![(0, 0x1100), (1, 0x1200)],
            2,
            false,
            Some(ProofFailureFamily::MissingOrdinalCoverage),
        );
        let decision =
            crate::midend::structuring::EmitReadyDecision::from_dispatcher_proof(Some(&proof));
        assert!(decision.proof_present);
        assert!(!decision.proof_complete);
        assert!(!decision.emit_ready);
        assert_eq!(
            decision.failure,
            Some(crate::midend::structuring::EmitReadyFailureFamily::ProofIncomplete)
        );
    }
}
