//! Individual P-code optimization passes.

mod copy_propagation;
mod cse;
mod dead_code;
pub mod def_use;
mod loop_header;
mod subvariable_flow;

pub use copy_propagation::CopyPropagator;
pub use cse::CommonSubexpressionEliminator;
pub use dead_code::DeadCodeEliminator;
pub use def_use::DefUseTracker;
pub use loop_header::LoopHeaderTempCoalescer;
pub use subvariable_flow::SubvariableFlowOptimizer;
