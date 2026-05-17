//! Core CFG analysis algorithms.

mod dominator;
mod loops;
mod metrics;

pub(super) use super::{CfgError, CfgResult, ControlFlowGraph, EdgeKind};
pub use dominator::DominatorTree;
pub use loops::{Loop, LoopAnalyzer, LoopKind};
pub use metrics::{CfgMetrics, ComplexityAnalyzer};
