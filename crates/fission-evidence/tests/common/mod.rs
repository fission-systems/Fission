//! A two-image world built from literal bytes: no file, loader or clock, so
//! every run of these fixtures sees the same inputs.

#![allow(dead_code)]

use fission_evidence::{
    ContentDigest, DecodeContext, ImageBytes, ImageIdentity, Mapping, MappingBacking, RunId,
    RuntimeLocation, SpecContext,
};

pub const IMAGE_SIZE: usize = 0x100;

pub struct Image {
    pub identity: ImageIdentity,
    pub bytes: Vec<u8>,
}

impl Image {
    pub fn new(fill: u8) -> Self {
        let bytes: Vec<u8> = (0..IMAGE_SIZE)
            .map(|i| fill.wrapping_add(i as u8))
            .collect();
        Self {
            identity: ImageIdentity {
                content: ContentDigest::of(&bytes),
                address_space: "ram".into(),
                spec: SpecContext {
                    language_id: "x86:LE:64:default".into(),
                    compiler_spec_id: Some("gcc".into()),
                },
            },
            bytes,
        }
    }

    pub fn mapped_at(&self, start: u64, from_epoch: u64) -> Mapping {
        Mapping {
            start,
            size: self.bytes.len() as u64,
            from_epoch,
            until_epoch: None,
            backing: MappingBacking::Image {
                image: self.identity.clone(),
                image_offset: 0,
            },
        }
    }

    pub fn at(&self, offset: usize, len: usize) -> &[u8] {
        &self.bytes[offset..offset + len]
    }
}

impl ImageBytes for Image {
    fn identity(&self) -> &ImageIdentity {
        &self.identity
    }

    fn bytes(&self, offset: u64, len: usize) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        self.bytes.get(start..start.checked_add(len)?)
    }
}

pub fn run(label: &str) -> RunId {
    RunId(ContentDigest::of(label.as_bytes()))
}

pub fn at(run_label: &str, epoch: u64, address: u64) -> RuntimeLocation {
    RuntimeLocation {
        run: run(run_label),
        process: 1,
        thread: 1,
        epoch,
        address,
    }
}

pub fn plain() -> DecodeContext {
    DecodeContext::default()
}
