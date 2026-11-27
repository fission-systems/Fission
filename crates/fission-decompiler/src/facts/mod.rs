//! Decompiler facts integration — type context assembly, call target resolution,
//! and debug-info (DWARF) ingestion.

mod facts;
pub mod single_flight;

// Re-export inner module contents at this level so that existing
// `crate::facts::build_nir_type_context` etc. paths continue to resolve.
pub use self::facts::callee_summary_single_flight_metrics;
pub(crate) use self::facts::*;
pub use self::single_flight::{
    ExecutionMode, FlightState, SingleFlightCache, SingleFlightError, SingleFlightMetricsSnapshot,
};

// Whole-program call-arity pre-analysis is the one entrypoint here meant to
// be driven from *outside* this crate (a caller's background
// discovery), so it needs a real `pub` re-export -- the blanket
// `pub(crate) use` above only reaches other modules within this crate.
pub use self::facts::seed_whole_program_call_arity_facts;

/// Exported for `bin/export_call_param_rules`: the static half of the call
/// parameter rules, which the runtime should load rather than rediscover.
pub use self::facts::name_keyed_call_param_rules;
