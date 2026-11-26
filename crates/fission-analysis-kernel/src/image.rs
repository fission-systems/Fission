//! A loaded binary seen as one image of the evidence contracts.

use crate::KernelError;
use fission_evidence::{
    ContentDigest, DecodeContext, ImageBytes, ImageIdentity, Mapping, MappingBacking, SpecContext,
};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::PackedContextOverride;

/// The address space the loader's code addresses are in.
const CODE_SPACE: &str = "ram";

/// Canonical text of a decoder context override.
pub fn decode_context(context: Option<PackedContextOverride>) -> DecodeContext {
    DecodeContext {
        context_override: context.map(|value| format!("{:x}/{:x}", value.context(), value.mask())),
    }
}

pub struct ImageView<'a> {
    binary: &'a LoadedBinary,
    identity: ImageIdentity,
}

impl<'a> ImageView<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Result<Self, KernelError> {
        let load_spec = binary.load_spec().ok_or(KernelError::NoLoadSpec)?;
        let identity = ImageIdentity {
            content: ContentDigest::from_blake3_hex(&binary.hash)?,
            address_space: CODE_SPACE.to_string(),
            spec: SpecContext {
                language_id: load_spec.pair.language_id.as_str().to_string(),
                compiler_spec_id: Some(load_spec.pair.compiler_spec_id.as_str().to_string()),
            },
        };
        Ok(Self { binary, identity })
    }

    pub fn binary(&self) -> &'a LoadedBinary {
        self.binary
    }

    /// Image-relative offset of a loader address.
    pub fn offset_of(&self, address: u64) -> Option<u64> {
        address.checked_sub(self.binary.image_base)
    }

    /// Loader address of an image-relative offset.
    pub fn address_of(&self, offset: u64) -> Option<u64> {
        offset.checked_add(self.binary.image_base)
    }

    /// Where the executable sections sit when the image is loaded at the
    /// loader's own addresses, in address order. A section overlapping an
    /// earlier one is left out: one address cannot show two offsets.
    pub fn code_mappings(&self) -> Vec<Mapping> {
        let mut sections: Vec<(u64, u64)> = self
            .binary
            .sections
            .iter()
            .filter(|section| section.is_executable)
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_size.max(section.file_size),
                )
            })
            .collect();
        sections.sort_unstable();
        let mut mappings = Vec::new();
        let mut covered_to = 0u64;
        for (start, size) in sections {
            let (Some(end), Some(image_offset)) = (start.checked_add(size), self.offset_of(start))
            else {
                continue;
            };
            if size == 0 || start < covered_to {
                continue;
            }
            covered_to = end;
            mappings.push(Mapping {
                start,
                size,
                from_epoch: 0,
                until_epoch: None,
                backing: MappingBacking::Image {
                    image: self.identity.clone(),
                    image_offset,
                },
            });
        }
        mappings
    }
}

impl ImageBytes for ImageView<'_> {
    fn identity(&self) -> &ImageIdentity {
        &self.identity
    }

    fn bytes(&self, offset: u64, len: usize) -> Option<&[u8]> {
        self.binary
            .view_executable_bytes(self.address_of(offset)?, len)
            .filter(|bytes| bytes.len() == len)
    }
}
