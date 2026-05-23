//! Decompiler orchestration — engine modes, routing decisions, worker dispatch,
//! failure taxonomy, and request/response contracts.

pub mod engine;
pub mod request;
pub mod routing;
pub mod taxonomy;
pub mod types;
pub mod worker;
