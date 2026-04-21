//! Clean-room mapping from Ghidra decompiler action concepts to Fission stages.
//!
//! This module intentionally records conceptual ownership only.  It does not
//! import, bind, or execute Ghidra code; it provides stable telemetry so the
//! Rust-native pipeline can be migrated stage-by-stage against the Ghidra
//! reference architecture.

use super::types::NirBuildStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GhidraActionConcept {
    FuncdataBuild,
    HeritageValueRecovery,
    Normalize,
    PrototypeTypes,
    BlockGraphStructuring,
    PrintC,
}

#[allow(dead_code)]
impl GhidraActionConcept {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            GhidraActionConcept::FuncdataBuild => "funcdata_build",
            GhidraActionConcept::HeritageValueRecovery => "heritage_value_recovery",
            GhidraActionConcept::Normalize => "normalize",
            GhidraActionConcept::PrototypeTypes => "prototype_types",
            GhidraActionConcept::BlockGraphStructuring => "blockgraph_structuring",
            GhidraActionConcept::PrintC => "printc",
        }
    }

    pub(crate) const fn ghidra_reference(self) -> &'static str {
        match self {
            GhidraActionConcept::FuncdataBuild => "Funcdata",
            GhidraActionConcept::HeritageValueRecovery => "Heritage",
            GhidraActionConcept::Normalize => "ActionNormalizeSetup / core Action pipeline",
            GhidraActionConcept::PrototypeTypes => "ActionPrototypeTypes / FuncProto",
            GhidraActionConcept::BlockGraphStructuring => {
                "FlowBlock / BlockGraph / ActionStructureTransform"
            }
            GhidraActionConcept::PrintC => "PrintC",
        }
    }

    pub(crate) const fn fission_owner(self) -> &'static str {
        match self {
            GhidraActionConcept::FuncdataBuild => "nir::builder",
            GhidraActionConcept::HeritageValueRecovery => "nir::builder::materialize",
            GhidraActionConcept::Normalize => "nir::normalize::pipeline",
            GhidraActionConcept::PrototypeTypes => "nir::normalize::types",
            GhidraActionConcept::BlockGraphStructuring => "nir::structuring",
            GhidraActionConcept::PrintC => "nir::printer",
        }
    }
}

#[allow(dead_code)]
pub(crate) const GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE: [GhidraActionConcept; 6] = [
    GhidraActionConcept::FuncdataBuild,
    GhidraActionConcept::HeritageValueRecovery,
    GhidraActionConcept::Normalize,
    GhidraActionConcept::PrototypeTypes,
    GhidraActionConcept::BlockGraphStructuring,
    GhidraActionConcept::PrintC,
];

pub(crate) fn record_ghidra_action_stage(stats: &mut NirBuildStats, concept: GhidraActionConcept) {
    stats.ghidra_action_stage_count += 1;
    match concept {
        GhidraActionConcept::FuncdataBuild => stats.ghidra_action_funcdata_build_count += 1,
        GhidraActionConcept::HeritageValueRecovery => {
            stats.ghidra_action_heritage_value_recovery_count += 1;
        }
        GhidraActionConcept::Normalize => stats.ghidra_action_normalize_count += 1,
        GhidraActionConcept::PrototypeTypes => stats.ghidra_action_prototype_types_count += 1,
        GhidraActionConcept::BlockGraphStructuring => {
            stats.ghidra_action_blockgraph_structuring_count += 1;
        }
        GhidraActionConcept::PrintC => stats.ghidra_action_printc_count += 1,
    }
}

pub(crate) fn record_ghidra_clean_room_pipeline_complete(stats: &mut NirBuildStats) {
    stats.ghidra_clean_room_pipeline_complete_count += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghidra_clean_room_action_sequence_is_stable() {
        let names: Vec<_> = GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE
            .iter()
            .map(|stage| stage.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "funcdata_build",
                "heritage_value_recovery",
                "normalize",
                "prototype_types",
                "blockgraph_structuring",
                "printc",
            ]
        );
    }

    #[test]
    fn ghidra_action_stage_recording_updates_exact_counter() {
        let mut stats = NirBuildStats::default();
        record_ghidra_action_stage(&mut stats, GhidraActionConcept::BlockGraphStructuring);
        assert_eq!(stats.ghidra_action_stage_count, 1);
        assert_eq!(stats.ghidra_action_blockgraph_structuring_count, 1);
        assert_eq!(stats.ghidra_action_funcdata_build_count, 0);
        assert_eq!(
            GhidraActionConcept::BlockGraphStructuring.ghidra_reference(),
            "FlowBlock / BlockGraph / ActionStructureTransform"
        );
        assert_eq!(
            GhidraActionConcept::BlockGraphStructuring.fission_owner(),
            "nir::structuring"
        );
    }
}
