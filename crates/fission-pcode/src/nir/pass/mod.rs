mod func;
mod manager;
mod store;
mod structuring;

pub(crate) use func::NirFunc;
pub(crate) use manager::{NirPass, PassManager, PassResult, RepeatMode};
pub(crate) use store::AnalysisStore;
pub(crate) use structuring::{
    EarlyReturnPass, IrreducibleReductionPass, OrphanGotoRepairPass, SeseStructuringPass,
};
