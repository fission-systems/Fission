//! Heritage stage boundary contract tests.

use fission_midend_dir::action_pipeline::{
    GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE, GhidraActionConcept, stage_boundary_violation,
};
use crate::pipeline::build_normalize_pipeline;

#[test]
fn heritage_concept_precedes_normalize_in_canonical_sequence() {
    let heritage_pos = GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE
        .iter()
        .position(|c| *c == GhidraActionConcept::HeritageValueRecovery)
        .expect("heritage stage");
    let normalize_pos = GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE
        .iter()
        .position(|c| *c == GhidraActionConcept::Normalize)
        .expect("normalize stage");
    assert!(heritage_pos < normalize_pos);
}

#[test]
fn normalize_pipeline_places_heritage_before_memory_recovery() {
    let pipeline = build_normalize_pipeline();
    let names = pipeline.group_names();
    let heritage = names
        .iter()
        .position(|name| *name == "heritage_value_recovery")
        .expect("heritage group");
    let memory = names
        .iter()
        .position(|name| *name == "memory_recovery")
        .expect("memory group");
    assert!(heritage < memory);
}

#[test]
fn stage_boundary_violation_rejects_heritage_pass_in_normalize_only_group() {
    assert!(stage_boundary_violation(
        GhidraActionConcept::Normalize,
        GhidraActionConcept::HeritageValueRecovery,
    ));
}

#[test]
fn stage_boundary_violation_allows_prototype_types_after_normalize() {
    assert!(!stage_boundary_violation(
        GhidraActionConcept::Normalize,
        GhidraActionConcept::PrototypeTypes,
    ));
}
