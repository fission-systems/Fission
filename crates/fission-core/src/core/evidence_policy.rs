//! Central numeric thresholds for identity and packed-binary evidence.
//!
//! Defaults match pre-refactor literals in `fission-loader` identity scoring/policy.
//! Confidence tiers are applied in the loader; this module holds numeric policy only.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdentityEvidenceThresholds {
    /// Scores `<= score_low_max` map to Low confidence.
    pub score_low_max: u32,
    /// Scores `<= score_medium_max` (and `> score_low_max`) map to Medium.
    pub score_medium_max: u32,
    /// Minimum distinct evidence sources required to keep High after score band allows it.
    pub high_min_distinct_sources: usize,
    /// Packer/protector kinds need `score >= this` to remain High (with sources satisfied).
    pub packer_protector_high_min_score: u32,
}

impl IdentityEvidenceThresholds {
    pub const DEFAULT: Self = Self {
        score_low_max: 3,
        score_medium_max: 5,
        high_min_distinct_sources: 2,
        packer_protector_high_min_score: 7,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedScorePolicy {
    pub entropy_weight_per_section: f32,
    pub entropy_section_cap: usize,
    pub packer_present_bump: f32,
    pub overlay_bump: f32,
    pub deduction_entry_in_text_section: f32,
    pub deduction_rich_import_table: f32,
    pub deduction_debug_present: f32,
    pub deduction_no_high_entropy_executable: f32,
    pub deduction_compiler_medium_or_high: f32,
}

impl PackedScorePolicy {
    pub const DEFAULT: Self = Self {
        entropy_weight_per_section: 0.15,
        entropy_section_cap: 3,
        packer_present_bump: 0.35,
        overlay_bump: 0.1,
        deduction_entry_in_text_section: 0.15,
        deduction_rich_import_table: 0.12,
        deduction_debug_present: 0.12,
        deduction_no_high_entropy_executable: 0.08,
        deduction_compiler_medium_or_high: 0.1,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuxiliaryEvidenceThresholds {
    pub import_table_rich_symbol_count: usize,
    pub weak_signal_score_cap: u32,
}

impl AuxiliaryEvidenceThresholds {
    pub const DEFAULT: Self = Self {
        import_table_rich_symbol_count: 16,
        weak_signal_score_cap: 3,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvidencePolicy {
    pub identity: IdentityEvidenceThresholds,
    pub packed: PackedScorePolicy,
    pub auxiliary: AuxiliaryEvidenceThresholds,
}

impl EvidencePolicy {
    pub const DEFAULT: Self = Self {
        identity: IdentityEvidenceThresholds::DEFAULT,
        packed: PackedScorePolicy::DEFAULT,
        auxiliary: AuxiliaryEvidenceThresholds::DEFAULT,
    };
}
