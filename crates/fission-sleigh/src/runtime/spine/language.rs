use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::compiler::{compile_frontend_for_entry_spec, CompiledFrontend, EntrySpec};
use crate::runtime::{registry, RuntimeFrontendStatus, RuntimeSleighError, UNIQUE_SPACE_ID};

#[derive(Debug, Clone)]
pub struct LanguageRuntime {
    pub profile: ProcessorRuntimeProfile,
    pub entry: EntrySpec,
    pub compiled: CompiledFrontend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorRuntimeProfile {
    pub ghidra_processor: String,
    pub module_name: String,
    pub entry_id: String,
    pub entry_spec: String,
    pub status: RuntimeFrontendStatus,
    pub endian: RuntimeEndian,
    pub addressable_unit_bytes: u8,
    pub unique_space_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeEndian {
    Little,
    Big,
    Unknown,
}

impl RuntimeEndian {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Little => "little",
            Self::Big => "big",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAttemptReport {
    pub processor: String,
    pub module_name: String,
    pub entry_id: String,
    pub entry_spec: String,
    pub status: RuntimeFrontendStatus,
    pub endian: RuntimeEndian,
    pub compiled_table_available: bool,
    pub constructor_inventory_count: usize,
    pub decision_node_count: usize,
    pub constructor_template_count: usize,
    pub unsupported_template_count: usize,
    pub executable_candidate: bool,
    pub fail_closed_reason: Option<String>,
}

impl LanguageRuntime {
    pub fn compile(entry: &EntrySpec) -> Result<Self> {
        let compiled = compile_frontend_for_entry_spec(&entry.path)?;
        let profile = ProcessorRuntimeProfile::from_entry(entry)?;
        Ok(Self {
            profile,
            entry: entry.clone(),
            compiled,
        })
    }

    pub fn attempt_report(&self) -> RuntimeAttemptReport {
        let unsupported_template_count = self
            .compiled
            .subtables
            .values()
            .flat_map(|s| s.constructors.iter())
            .filter(|constructor| constructor.unsupported_template_kind.is_some())
            .count();
        let executable_candidate =
            self.profile.status == RuntimeFrontendStatus::ExecutableCandidate;
        RuntimeAttemptReport {
            processor: self.profile.ghidra_processor.clone(),
            module_name: self.profile.module_name.clone(),
            entry_id: self.entry.entry_id.clone(),
            entry_spec: self.entry.entry_spec.clone(),
            status: self.profile.status,
            endian: self.profile.endian,
            compiled_table_available: true,
            constructor_inventory_count: self.compiled.constructors.len(),
            decision_node_count: self
                .compiled
                .subtables
                .values()
                .map(|s| s.decision_tree.decision_node_count)
                .sum(),
            constructor_template_count: self
                .compiled
                .subtables
                .values()
                .map(|s| s.constructors.len())
                .sum(),
            unsupported_template_count,
            executable_candidate,
            fail_closed_reason: (!executable_candidate).then(|| {
                "registered compile-only processor has no promoted runtime consumer".to_string()
            }),
        }
    }

    pub fn unsupported_decode_error(&self) -> RuntimeSleighError {
        RuntimeSleighError::UnsupportedGeneratedSemantic {
            language: self.entry.entry_id.clone(),
            status: self.profile.status,
        }
    }
}

impl ProcessorRuntimeProfile {
    pub fn from_entry(entry: &EntrySpec) -> Result<Self> {
        let variant = registry::runtime_variant_for_entry(entry)?;
        Ok(Self {
            ghidra_processor: variant.processor,
            module_name: variant.module_name,
            entry_id: entry.entry_id.clone(),
            entry_spec: entry.entry_spec.clone(),
            status: variant.support_level.as_frontend_status(),
            endian: variant.endian,
            addressable_unit_bytes: 1,
            unique_space_id: UNIQUE_SPACE_ID,
        })
    }
}
