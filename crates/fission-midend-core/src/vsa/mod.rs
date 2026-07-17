/// Value Set Analysis (VSA) module.
///
/// Provides abstract-interpretation-based range analysis for HIR functions,
/// with applications to:
/// - Switch/indirect jump resolution (case pruning, singleton elimination)
/// - Dead branch removal
/// - Loop bound detection
///
/// ## Architecture
///
/// ```text
/// circle_range.rs  — CircleRange domain (wrapping intervals mod 2^n)
/// transfer.rs      — HirExpr → CircleRange transfer functions
/// solver.rs        — Worklist fixed-point solver with widening
/// jump_resolver.rs — Switch refinement using VSA results
/// ```
pub mod circle_range;
pub mod jump_resolver;
pub mod solver;
pub mod transfer;

pub use jump_resolver::{apply_jump_resolver_pass, jump_resolver_candidate_count};
