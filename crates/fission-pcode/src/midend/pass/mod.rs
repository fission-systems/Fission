mod func;
mod manager;
mod store;
mod structuring;

pub(crate) use func::NirFunc;
pub(crate) use manager::{
    InvariantBasis, MAX_STRUCTURING_PASSES, NirPass, PassManager, PassResult, PassRunError,
    RepeatMode, validate_nir_func,
};
pub(crate) use store::AnalysisStore;
pub(crate) use structuring::{
    EarlyReturnPass, IrreducibleReductionPass, OrphanGotoRepairPass, SeseStructuringPass,
};
