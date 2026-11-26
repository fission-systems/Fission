//! Where in an image, and which bytes were there.

use crate::digest::ContentDigest;
use crate::identity::ImageIdentity;
use serde::{Deserialize, Serialize};

/// Decoder state that changes what the bytes at an address mean.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct DecodeContext {
    /// SLEIGH context register override, for example ARM's Thumb mode. `None`
    /// is the language's default context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_register: Option<u32>,
}

/// The version of a stretch of code: its length and the digest of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CodeVersion {
    pub len: u32,
    pub digest: ContentDigest,
}

impl CodeVersion {
    /// `None` for an empty stretch, which versions nothing.
    pub fn of(bytes: &[u8]) -> Option<Self> {
        let len = u32::try_from(bytes.len()).ok().filter(|len| *len > 0)?;
        Some(Self {
            len,
            digest: ContentDigest::of(bytes),
        })
    }

    pub fn matches(&self, bytes: &[u8]) -> bool {
        Self::of(bytes).as_ref() == Some(self)
    }
}

/// Canonical code location. It stays valid across load addresses, runs and
/// snapshot rebuilds, and stops applying when the bytes or decode context do.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CodeLocation {
    pub image: ImageIdentity,
    /// Address minus the image base.
    pub offset: u64,
    /// The bytes at `offset`.
    pub version: CodeVersion,
    #[serde(default)]
    pub decode: DecodeContext,
}

/// Natural identity of a function: where it enters in which image.
///
/// A `ProgramSnapshot` row id is not identity. It is an index into one snapshot
/// and never appears in a contract that outlives that snapshot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FunctionKey {
    pub image: ImageIdentity,
    pub entry_offset: u64,
    #[serde(default)]
    pub decode: DecodeContext,
}
