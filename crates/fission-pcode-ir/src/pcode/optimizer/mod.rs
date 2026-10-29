//! P-code optimizer driver and pass bundle.

mod driver;
pub mod passes;
mod rules;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dead_bit;

pub use driver::{PcodeOptimizer, PcodeOptimizerConfig};
pub use passes::{
    CommonSubexpressionEliminator, CopyPropagator, DeadCodeEliminator, DefUseTracker,
    LoopHeaderTempCoalescer, SubvariableFlowOptimizer,
};
pub use rules::OptimizationRules;

#[cfg(test)]
pub(crate) use crate::pcode::{PcodeBasicBlock, PcodeFunction};
