//! Orchestration over the analysis engines (ADR 0020, issue #1368).
//!
//! The kernel owns no analysis. It turns what one engine reports into
//! `fission-evidence` contracts and joins them with what another engine knows:
//!
//! - [`record`] (feature `emulator`): watch a run and write a runtime trace;
//! - [`import`]: validate a trace against a binary and add its claims;
//! - [`view`]: a function's indirect sites with candidate targets and their
//!   provenance;
//! - [`store`]: the ledger on disk.
//!
//! Everything here is opt-in. Ordinary decompilation reads no evidence, and
//! nothing here changes or prunes what the decompiler recovers.

pub mod image;
pub mod import;
#[cfg(feature = "emulator")]
pub mod record;
pub mod store;
pub mod view;

pub use image::ImageView;
pub use import::{ImportReport, import_trace};
pub use view::indirect_targets_view;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("the binary has no load specification, so its code has no identity")]
    NoLoadSpec,
    #[error("the binary's content hash is not usable: {0}")]
    ContentHash(#[from] fission_evidence::DigestError),
    #[error(
        "the trace was recorded for another image: this binary is {expected}, the trace names {found}"
    )]
    IncompatibleImage { expected: String, found: String },
    #[error("the trace's mappings are invalid: {0}")]
    Mapping(#[from] fission_evidence::MappingError),
    #[error("transfer {record} of the trace has a zero count")]
    EmptyTransfer { record: usize },
    #[error(transparent)]
    Trace(#[from] fission_evidence::TraceError),
    #[error(transparent)]
    Ledger(#[from] fission_evidence::LedgerError),
    #[error("cannot lift the function: {0}")]
    Lift(String),
    #[error("the program snapshot cannot be published: {0}")]
    Snapshot(String),
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}
