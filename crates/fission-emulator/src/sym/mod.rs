pub mod state;
pub mod manager;
pub mod exploration;

pub use state::{SimState, SimStateHistory};
pub use manager::SimulationManager;

/// Alias for CLI compatibility — `SymbolicExecutor` is the public-facing name
/// for the TTD-backed concolic exploration engine (internally `SimulationManager`).
pub use manager::SimulationManager as SymbolicExecutor;
