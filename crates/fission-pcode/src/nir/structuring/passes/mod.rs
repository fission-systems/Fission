//! Structuring stage exposed as a typed `Pass` pipeline.
//!
//! Each structuring rule (loop lowering, conditional lowering, switch lowering)
//! is registered as an explicit [`crate::nir::action_pipeline::Pass`] implementation
//! so that the `PassManager` (ActionGroup with `Repeat::UntilStable`) controls
//! fixed-point iteration and budget, rather than an ad-hoc internal loop.
//!
//! This prevents "round-about overfitting patches" — any change that bypasses
//! the Pass boundary will fail to compile because structuring state is not
//! accessible outside the Pass interface.

pub(crate) mod pipeline;

pub(crate) use pipeline::build_structuring_pipeline;
