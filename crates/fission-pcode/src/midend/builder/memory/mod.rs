//! Stack, aggregate, and memory-surface recovery helpers.

pub(super) use super::*;

pub(in crate::midend::builder) mod aggregate_recovery;
mod stack_slots;
