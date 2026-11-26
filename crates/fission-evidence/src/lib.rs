//! Contracts for evidence that crosses analysis layers (ADR 0020).
//!
//! Static analysis, the decompiler, the emulator and the solver each know
//! things about a binary. This crate says how such a thing is named so that it
//! can move between them without being attributed to the wrong code:
//!
//! - [`identity`]: which image, under which decode specification;
//! - [`location`]: where in that image, and which version of the bytes;
//! - [`runtime`]: where in a running process, and how that maps back to an
//!   image -- or does not, for modified and anonymous code;
//! - [`evidence`]: who claims what, within which scope, from which inputs;
//! - [`ledger`]: the append-only collection of those claims.
//!
//! The crate holds contracts and their validation. It reads no binary, runs no
//! analysis and persists nothing: producers and consumers own that.

pub mod digest;
pub mod evidence;
pub mod identity;
pub mod ledger;
pub mod location;
pub mod runtime;

pub use digest::{ContentDigest, DigestError};
pub use evidence::{
    ArtifactRef, Dependency, Entity, Evidence, EvidenceError, EvidenceId, Lifecycle, Predicate,
    Producer, Scope, SourceKind, Value, VerificationStatus,
};
pub use identity::{ImageIdentity, SpecContext};
pub use ledger::{
    EVIDENCE_LEDGER_SCHEMA, EvidenceLedger, InsertOutcome, LedgerError, StatusChange, StatusEvent,
};
pub use location::{CodeLocation, CodeVersion, DecodeContext, FunctionKey};
pub use runtime::{
    Attribution, ImageBytes, Mapping, MappingBacking, MappingError, MappingTable, RunId,
    RuntimeCodeLocation, RuntimeCodeOrigin, RuntimeLocation, UnattributedReason, attribute,
};
