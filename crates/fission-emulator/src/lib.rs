pub mod arch;
pub mod core;
pub mod jit;
pub mod loader;
pub mod metrics;
pub mod os;
pub mod pcode;
pub mod snapshot;
pub mod sym;
pub mod trace;
// Unix-only: `codebuf.rs`'s executable-memory management is written
// directly against `libc::mmap`/`mprotect`/`munmap`, which don't exist on
// Windows (`libc`'s Windows build has none of the POSIX mmap API). selfjit
// is an experimental, not-yet-wired-in JIT backend (see its own module doc)
// with no callers outside this module, so gating the whole thing out on
// Windows rather than porting `codebuf.rs` to `VirtualAlloc`/`VirtualProtect`
// is the right scope for now.
#[cfg(unix)]
pub mod selfjit;
pub mod srd;

pub use arch::{ArchInfo, Endianness};
pub use core::{Emulator, RunOutcome};
pub use metrics::{BudgetReport, EmulatorMetrics, SandboxMetricsReport};
pub use os::{BareMetalEnv, HleResult, LinuxEnv, OsEnvironment, WindowsEnv};
pub use pcode::eval::Evaluator;
pub use pcode::state::MachineState;
pub use snapshot::EmulatorSnapshot;
pub use srd::{
    CaptureOpts, FieldDelta, MallocngProbe, OwnerLayer, SemanticReplayDelta, SemanticReplaySnapshot,
};
