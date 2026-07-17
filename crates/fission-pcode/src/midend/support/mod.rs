//! Internal support types and helpers for the NIR pipeline.
//!
//! Re-exports everything so existing `use super::*` imports continue to work.

pub(super) use super::*;

mod builder_types;
mod emulate;
mod expr_util;
mod pcode_util;
mod switch_util;

pub(crate) use builder_types::*;
pub(crate) use emulate::*;
pub(crate) use expr_util::*;
pub(crate) use pcode_util::*;
pub(crate) use switch_util::*;
