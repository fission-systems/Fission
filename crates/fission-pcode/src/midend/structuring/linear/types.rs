//! Type definitions for linear body lowering — re-exported from midend-structuring.

pub use fission_midend_structuring::{
    ConditionalTailLoweringResult, ConditionalTailMismatchSubtype, LinearBodyCachedOutcome,
    LinearBodyLoweringOutcome, LinearBodyRejectReason, MAX_LINEAR_STRUCTURING_DEPTH,
    NormalizedConditionalTailArm,
};

pub use fission_midend_structuring::linear_types::{
    MAX_REGION_FOLLOW_DISCOVERY_STEPS, MAX_REGION_JOIN_TRAMPOLINE_DISTANCE,
    MAX_REGION_SHARED_TAIL_STEPS, MAX_REGION_TARGET_CANONICALIZE_STEPS,
};
