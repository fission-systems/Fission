//! Time Travel Debugging (TTD) Module
//!
//! Provides the ability to record program execution and "rewind" to previous states.
//!
//! # Architecture
//! - **Snapshot**: Captures complete state at a point in time
//! - **Recorder**: Records execution step by step
//! - **Timeline**: Manages recorded history and navigation

pub mod snapshot;
pub mod recorder;
pub mod timeline;

pub use snapshot::{ExecutionSnapshot, MemoryDelta};
pub use recorder::TTDRecorder;
pub use timeline::Timeline;
