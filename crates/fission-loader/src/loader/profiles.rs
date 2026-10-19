//! Owned loader/profile selection, independent of Ghidra language identifiers.
//! Policies are caller supplied; parsing/mapping stays in canonical format loaders.
use super::{LoadedBinary, pipeline::DetectedFormat, reader::Endian};
use fission_core::architecture::ArchitectureDescriptor;
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinaryTargetFacts {
    pub format: DetectedFormat,
    pub machine: u16,
    pub bitness: u8,
    pub endian: Endian,
    pub flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedProfile {
    pub id: String,
    pub format: DetectedFormat,
    pub machine: u16,
    pub bitness: u8,
    pub endian: Endian,
    pub flags_mask: u32,
    pub flags_value: u32,
    pub processor: String,
    pub variant: String,
    pub decoder: String,
    /// Identity of the compiled semantic/encoding package, checked by its consumer.
    pub package_sha256: String,
}

impl OwnedProfile {
    pub fn matches(&self, facts: &BinaryTargetFacts) -> bool {
        self.format == facts.format
            && self.machine == facts.machine
            && self.bitness == facts.bitness
            && self.endian == facts.endian
            && facts.flags & self.flags_mask == self.flags_value
    }

    pub(crate) fn architecture(&self, facts: &BinaryTargetFacts) -> ArchitectureDescriptor {
        ArchitectureDescriptor::new(
            &self.processor,
            if facts.endian == Endian::Little {
                "little"
            } else {
                "big"
            },
            facts.bitness,
            &self.variant,
            None,
            format!(
                "owned-profile={} ELF machine={} flags=0x{:x}",
                self.id, facts.machine, facts.flags
            ),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileSelectionError {
    InvalidCatalog(String),
    NoMatch(BinaryTargetFacts),
    Ambiguous(Vec<String>),
}
impl fmt::Display for ProfileSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCatalog(message) => write!(f, "InvalidOwnedCatalog: {message}"),
            Self::NoMatch(facts) => write!(f, "OwnedProfileNotFound: {facts:?}"),
            Self::Ambiguous(ids) => write!(f, "AmbiguousOwnedProfiles: {ids:?}"),
        }
    }
}
impl std::error::Error for ProfileSelectionError {}

pub fn valid_profile_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

/// Validated, immutable catalog. Overlapping rules refuse; source order is not priority.
#[derive(Clone, Debug)]
pub struct OwnedProfileCatalog {
    profiles: Vec<OwnedProfile>,
}
impl OwnedProfileCatalog {
    pub fn new(profiles: Vec<OwnedProfile>) -> Result<Self, ProfileSelectionError> {
        let invalid = |message: &str| ProfileSelectionError::InvalidCatalog(message.into());
        if profiles.is_empty() || profiles.len() > 1024 {
            return Err(invalid("requires 1..1024 profiles"));
        }
        let mut names = BTreeSet::new();
        for (index, profile) in profiles.iter().enumerate() {
            if [
                &profile.id,
                &profile.processor,
                &profile.variant,
                &profile.decoder,
            ]
            .iter()
            .any(|s| !valid_profile_identifier(s))
                || !names.insert(profile.id.as_str())
                || profile.format != DetectedFormat::Elf
                || !matches!(profile.bitness, 32 | 64)
                || profile.flags_value & !profile.flags_mask != 0
                || profile.package_sha256.len() != 64
                || !profile
                    .package_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(invalid(
                    "invalid/duplicate identity, unsupported format, width, flag mask or package hash",
                ));
            }
            for other in &profiles[..index] {
                if profile.format == other.format
                    && profile.machine == other.machine
                    && profile.bitness == other.bitness
                    && profile.endian == other.endian
                    && (profile.flags_value ^ other.flags_value)
                        & (profile.flags_mask & other.flags_mask)
                        == 0
                {
                    return Err(ProfileSelectionError::Ambiguous(vec![
                        other.id.clone(),
                        profile.id.clone(),
                    ]));
                }
            }
        }
        Ok(Self { profiles })
    }
    pub fn profiles(&self) -> &[OwnedProfile] {
        &self.profiles
    }
    pub fn select(
        &self,
        facts: &BinaryTargetFacts,
    ) -> Result<&OwnedProfile, ProfileSelectionError> {
        self.profiles
            .iter()
            .find(|profile| profile.matches(facts))
            .ok_or(ProfileSelectionError::NoMatch(*facts))
    }
}

/// Loading decision around the canonical binary payload, not a second program-fact store.
/// ABI remains unselected; a profile match does not recover a calling convention.
#[derive(Debug)]
pub struct ProfiledBinary {
    pub binary: LoadedBinary,
    pub target_facts: BinaryTargetFacts,
    pub profile: OwnedProfile,
}
