//! Expression and varnode lowering.

pub(super) use super::*;

mod call;
mod call_target;
mod lower_expr;
mod op_lowering;
mod register_alias;
