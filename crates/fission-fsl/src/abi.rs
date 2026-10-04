//! FSL-owned compiler/ABI metadata. This is not a parameter allocation engine.
use crate::{Evidence, FslError};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiRegisterEntry {
    pub register: String,
    pub min_bytes: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiMemoryEffect {
    pub space: String,
    pub offset: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiConvention {
    pub name: String,
    /// None preserves the source's explicit unknown cleanup amount.
    pub extrapop: Option<u64>,
    pub stackshift: u64,
    pub inputs: Vec<AbiRegisterEntry>,
    pub outputs: Vec<AbiRegisterEntry>,
    pub output_killed_by_call: bool,
    pub preserved_registers: Vec<String>,
    pub clobbered_registers: Vec<String>,
    pub preserved_memory: Vec<AbiMemoryEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiProfile {
    pub name: String,
    pub evidence: Vec<Evidence>,
    /// Source names retain byte units; alignment limits may explicitly be zero.
    pub data: BTreeMap<String, u64>,
    pub size_alignments: BTreeMap<u64, u64>,
    pub global_spaces: Vec<String>,
    pub stack_register: String,
    pub stack_space: String,
    pub default_convention: String,
    pub conventions: Vec<AbiConvention>,
}

impl AbiProfile {
    pub fn validate(&self) -> Result<(), FslError> {
        let error = |message| FslError::at(1, 1, message);
        if self.name.is_empty()
            || self.evidence.is_empty()
            || self.stack_register.is_empty()
            || self.stack_space.is_empty()
            || self.conventions.is_empty()
        {
            return Err(error(
                "ABI profile requires identity, provenance, stack and conventions",
            ));
        }
        for evidence in &self.evidence {
            if [
                &evidence.source_id,
                &evidence.url,
                &evidence.revision,
                &evidence.claim,
            ]
            .iter()
            .any(|s| s.is_empty())
            {
                return Err(error("ABI evidence cannot contain empty fields"));
            }
        }
        let pointer = self.data.get("pointer_size").copied().unwrap_or(0);
        if pointer == 0 || pointer > 512 {
            return Err(error(
                "ABI pointer size must be explicit and within 1..512 bytes",
            ));
        }
        if self
            .size_alignments
            .iter()
            .any(|(&size, &align)| size == 0 || align == 0)
        {
            return Err(error("ABI size alignment entries must be nonzero"));
        }
        let mut names = HashSet::new();
        for convention in &self.conventions {
            if convention.name.is_empty() || !names.insert(&convention.name) {
                return Err(error("ABI convention names must be nonempty and unique"));
            }
            for entry in convention.inputs.iter().chain(&convention.outputs) {
                if entry.register.is_empty()
                    || entry.min_bytes == 0
                    || entry.min_bytes > entry.max_bytes
                {
                    return Err(error("ABI register entry has invalid size bounds"));
                }
            }
            if convention
                .preserved_memory
                .iter()
                .any(|m| m.space.is_empty() || m.size_bytes == 0)
            {
                return Err(error("ABI memory effect requires a space and nonzero size"));
            }
            if convention
                .preserved_registers
                .iter()
                .chain(&convention.clobbered_registers)
                .any(|s| s.is_empty())
            {
                return Err(error("ABI register effect cannot have an empty name"));
            }
        }
        if !names.contains(&self.default_convention) {
            return Err(error("ABI default convention is not defined"));
        }
        Ok(())
    }
}

/// Compile the current strict ABI metadata subset; unknown syntax is rejected.
/// Register names remain symbolic until linked to an FSL register layout.
pub fn compile_abi_source(source: &str) -> Result<AbiProfile, FslError> {
    let profile = crate::parser::parse_abi(source)?;
    profile.validate()?;
    Ok(profile)
}
