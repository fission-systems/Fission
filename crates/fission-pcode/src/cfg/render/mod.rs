//! CFG rendering helpers.

mod visualization;

pub(super) use super::{BasicBlock, ControlFlowGraph, EdgeKind, Loop};
pub use visualization::{CfgVisualizer, DotOptions};
