pub mod exploration;
pub mod manager;
pub mod state;

pub use manager::SimulationManager;
pub use state::{SimState, SimStateHistory};

/// Alias for CLI compatibility — `SymbolicExecutor` is the public-facing name
/// for the TTD-backed concolic exploration engine (internally `SimulationManager`).
pub use manager::SimulationManager as SymbolicExecutor;
