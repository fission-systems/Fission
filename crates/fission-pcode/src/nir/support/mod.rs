//! Internal support types and helpers for the NIR pipeline.
//!
//! Re-exports everything so existing `use super::*` imports continue to work.

pub(super) use super::*;

mod builder_types;
mod calling_convention;
mod expr_util;
mod pcode_util;
mod register_map;
mod switch_util;

pub(crate) use builder_types::*;
pub use calling_convention::CallingConvention;
pub(crate) use calling_convention::*;
pub(crate) use expr_util::*;
pub(crate) use pcode_util::*;
pub(crate) use register_map::*;
pub(crate) use switch_util::*;
