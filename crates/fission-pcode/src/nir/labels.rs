//! Shared label / sentinel strings used across structuring and print surfaces.
//!
//! Substrate-level constants (ADR 0008): owners must not invent divergent copies.

/// Switch-case fallthrough edge marker. Structuring emits `Goto` to this label;
/// the printer renders it as a fallthrough comment rather than a real jump.
pub(crate) const SWITCH_FALLTHROUGH_SENTINEL: &str = "__fallthrough";
