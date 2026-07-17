//! Pure CFG analysis facts (dom, postdom, SCC, follow, trace DAG).

mod dom;
mod edge;
mod follow;
mod goto_selector;
mod postdom;
mod scc;
// tests re-enabled after build_predecessor helpers land in this crate
// #[cfg(test)]
// mod tests;
mod trace_dag;
pub mod util;

pub use dom::{DomTree, DominanceFrontier, ImmDomTree};
pub use edge::{CfgAnalysis, EdgeClass};
pub use follow::dom_based_fallthrough_successor;
pub use goto_selector::select_bad_edge;
pub use postdom::{ImmPostDomTree, PostDomTree};
pub use scc::SccAnalysis;
pub use trace_dag::{TraceDag, TraceDagError};
