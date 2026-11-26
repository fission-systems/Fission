//! Runtime addresses, and what they are allowed to be attributed to.
//!
//! A virtual address means something only inside one run, one process and one
//! mapping epoch. It becomes a [`CodeLocation`] only through a mapping that was
//! recorded for that epoch *and* bytes that equal the image's. Subtracting a
//! load base is not enough: code that was patched, unpacked or generated at run
//! time sits at an image address without being the image's code.

use crate::digest::ContentDigest;
use crate::identity::ImageIdentity;
use crate::location::{CodeLocation, CodeVersion, DecodeContext};
use serde::{Deserialize, Serialize};

/// Identity of one execution, derived by its producer from what was run and
/// what was observed. It carries no clock or random part, so replaying the same
/// execution names the same run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub ContentDigest);

/// An address as observed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeLocation {
    pub run: RunId,
    pub process: u32,
    pub thread: u32,
    /// Mapping epoch: it advances whenever the process's mappings change.
    pub epoch: u64,
    pub address: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MappingBacking {
    /// The mapping shows `image` from `image_offset` on.
    Image {
        image: ImageIdentity,
        image_offset: u64,
    },
    /// Memory with no image behind it: heap, stack, JIT buffers.
    Anonymous,
}

/// `[start, start + size)` of one process during `[from_epoch, until_epoch)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Mapping {
    pub start: u64,
    pub size: u64,
    #[serde(default)]
    pub from_epoch: u64,
    /// First epoch in which the mapping is gone. `None` while it is still live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_epoch: Option<u64>,
    pub backing: MappingBacking,
}

impl Mapping {
    fn last_address(&self) -> Option<u64> {
        self.size
            .checked_sub(1)
            .and_then(|n| self.start.checked_add(n))
    }

    fn live_in(&self, epoch: u64) -> bool {
        epoch >= self.from_epoch && self.until_epoch.is_none_or(|until| epoch < until)
    }

    fn shares_an_epoch_with(&self, other: &Self) -> bool {
        self.until_epoch
            .is_none_or(|until| other.from_epoch < until)
            && other
                .until_epoch
                .is_none_or(|until| self.from_epoch < until)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingError {
    #[error("mapping at 0x{start:x} is empty or runs past the end of the address space")]
    InvalidRange { start: u64 },
    #[error("mapping at 0x{start:x} lives for no epoch")]
    InvalidEpochs { start: u64 },
    #[error("mapping at 0x{start:x} overlaps the mapping at 0x{other:x} in a shared epoch")]
    Overlap { start: u64, other: u64 },
    #[error("no mapping starting at 0x{start:x} is live in epoch {epoch}")]
    NotMapped { start: u64, epoch: u64 },
}

/// The mapping history of one process. Every entry is validated on the way in,
/// so a lookup has at most one answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MappingTable {
    mappings: Vec<Mapping>,
}

impl MappingTable {
    pub fn new(mappings: impl IntoIterator<Item = Mapping>) -> Result<Self, MappingError> {
        let mut table = Self::default();
        for mapping in mappings {
            table.map(mapping)?;
        }
        Ok(table)
    }

    pub fn map(&mut self, mapping: Mapping) -> Result<(), MappingError> {
        let start = mapping.start;
        let last = mapping
            .last_address()
            .ok_or(MappingError::InvalidRange { start })?;
        if let MappingBacking::Image { image_offset, .. } = &mapping.backing
            && image_offset.checked_add(mapping.size - 1).is_none()
        {
            return Err(MappingError::InvalidRange { start });
        }
        if mapping
            .until_epoch
            .is_some_and(|until| until <= mapping.from_epoch)
        {
            return Err(MappingError::InvalidEpochs { start });
        }
        if let Some(other) = self.mappings.iter().find(|other| {
            other.shares_an_epoch_with(&mapping)
                && other.start <= last
                && other.last_address().is_some_and(|end| start <= end)
        }) {
            return Err(MappingError::Overlap {
                start,
                other: other.start,
            });
        }
        let at = self.mappings.partition_point(|other| other < &mapping);
        self.mappings.insert(at, mapping);
        Ok(())
    }

    /// End the mapping at `start`: it is gone from `epoch` on.
    pub fn unmap(&mut self, start: u64, epoch: u64) -> Result<(), MappingError> {
        let mapping = self
            .mappings
            .iter_mut()
            .find(|m| m.start == start && m.live_in(epoch) && epoch > m.from_epoch)
            .ok_or(MappingError::NotMapped { start, epoch })?;
        mapping.until_epoch = Some(epoch);
        Ok(())
    }

    pub fn lookup(&self, address: u64, epoch: u64) -> Option<&Mapping> {
        self.mappings
            .iter()
            .find(|m| m.live_in(epoch) && address >= m.start && address - m.start < m.size)
    }

    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }
}

/// Read access to one image's bytes, implemented by whoever loaded it.
pub trait ImageBytes {
    fn identity(&self) -> &ImageIdentity;
    /// Exactly `len` bytes at `offset` from the image base, or `None`.
    fn bytes(&self, offset: u64, len: usize) -> Option<&[u8]>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RuntimeCodeOrigin {
    /// Code in memory no image backs.
    Anonymous,
    /// Code at an image's place whose bytes are not the image's.
    ModifiedImage { image: ImageIdentity, offset: u64 },
}

/// Code that exists only in a run. Its version is part of its name, so two
/// generations of a JIT buffer are two entities, and neither is the file's code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeCodeLocation {
    pub at: RuntimeLocation,
    pub origin: RuntimeCodeOrigin,
    pub version: CodeVersion,
    #[serde(default)]
    pub decode: DecodeContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnattributedReason {
    /// No mapping covers the address in that epoch.
    Unmapped,
    /// The observed bytes run past the mapping that covers their start.
    CrossesMapping,
    /// The producer could not read the bytes, so nothing can be validated.
    NoBytes,
    /// The mapping shows an image other than the one being compared with.
    ForeignImage,
    /// The image has no bytes at that offset to compare with.
    ImageBytesUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Attribution {
    /// Validated: this is the image's own code.
    Image(CodeLocation),
    /// Code of this run only.
    RuntimeCode(RuntimeCodeLocation),
    /// Nothing may be concluded beyond the address itself.
    Unattributed {
        at: RuntimeLocation,
        reason: UnattributedReason,
    },
}

/// Normalize the code observed at `at` against one image.
///
/// `observed` is what was at the address when it was seen. Anything that cannot
/// be validated stays unattributed or runtime-only; it is never folded into the
/// image.
pub fn attribute(
    at: RuntimeLocation,
    observed: &[u8],
    decode: DecodeContext,
    mappings: &MappingTable,
    image: &dyn ImageBytes,
) -> Attribution {
    let unattributed = |at, reason| Attribution::Unattributed { at, reason };
    let Some(mapping) = mappings.lookup(at.address, at.epoch) else {
        return unattributed(at, UnattributedReason::Unmapped);
    };
    let Some(version) = CodeVersion::of(observed) else {
        return unattributed(at, UnattributedReason::NoBytes);
    };
    let delta = at.address - mapping.start;
    if u64::from(version.len) > mapping.size - delta {
        return unattributed(at, UnattributedReason::CrossesMapping);
    }
    match &mapping.backing {
        MappingBacking::Anonymous => Attribution::RuntimeCode(RuntimeCodeLocation {
            at,
            origin: RuntimeCodeOrigin::Anonymous,
            version,
            decode,
        }),
        MappingBacking::Image {
            image: mapped,
            image_offset,
        } => {
            if mapped != image.identity() {
                return unattributed(at, UnattributedReason::ForeignImage);
            }
            // `map` checked that every offset inside the mapping is in range.
            let offset = image_offset + delta;
            let Some(on_disk) = image.bytes(offset, observed.len()) else {
                return unattributed(at, UnattributedReason::ImageBytesUnavailable);
            };
            if on_disk == observed {
                Attribution::Image(CodeLocation {
                    image: mapped.clone(),
                    offset,
                    version,
                    decode,
                })
            } else {
                Attribution::RuntimeCode(RuntimeCodeLocation {
                    at,
                    origin: RuntimeCodeOrigin::ModifiedImage {
                        image: mapped.clone(),
                        offset,
                    },
                    version,
                    decode,
                })
            }
        }
    }
}
