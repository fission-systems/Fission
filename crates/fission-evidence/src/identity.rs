//! Which image a fact is about.

use crate::digest::ContentDigest;
use serde::{Deserialize, Serialize};

/// The specification the bytes were decoded under.
///
/// The same bytes lifted for another language or compiler spec are other code,
/// so both are part of image identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SpecContext {
    /// Ghidra language id. It names processor, endianness, size and variant,
    /// which is the architecture.
    pub language_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_spec_id: Option<String>,
}

/// One loaded image: its file content, the address space its offsets are in and
/// the specification it is decoded with.
///
/// Neither a path nor a load address is identity: a renamed or relocated image
/// is the same image, and a rebuilt file at the same path is not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ImageIdentity {
    /// Digest of the file's bytes.
    pub content: ContentDigest,
    /// SLEIGH address space of code offsets, `ram` for ordinary images.
    pub address_space: String,
    pub spec: SpecContext,
}
