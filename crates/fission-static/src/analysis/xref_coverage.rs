//! Machine-readable coverage for xref extraction.
//!
//! The report describes only the bounded input domain named by each layer's
//! `scope`; `complete_for_scope` is not a claim of whole-binary completeness.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefAnalysisLayer {
    Loader,
    Relocation,
    SymbolTable,
    Disassembly,
    Pcode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefAnalysisState {
    NotRequested,
    CompleteForScope,
    Partial,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefCoverageUnit {
    LoaderFact,
    ExecutableOrPointerDataSection,
    DiscoveredFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefOmissionReason {
    SectionBytesUnavailable,
    InstructionDecodeFailed,
    UndecodedExecutableTail,
    MissingFunctionExtent,
    FunctionOverByteLimit,
    FunctionSizeUnrepresentable,
    FunctionBytesUnavailable,
    FunctionLiftFailed,
    FunctionLiftInputExhausted,
    FunctionLiftInstructionLimit,
    FunctionLiftNotTerminal,
    ValueSetAnalysisIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefUnsupportedReason {
    LoadSpecUnavailable,
    SleighFrontendUnavailable,
    RamSpaceUnavailable,
    InvalidRamAddressableUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct XrefLayerCoverage {
    pub layer: XrefAnalysisLayer,
    pub status: XrefAnalysisState,
    /// The exact bounded input domain to which this status applies.
    pub scope: String,
    pub unit: XrefCoverageUnit,
    pub candidate_units: usize,
    pub completed_units: usize,
    pub omitted_units: usize,
    pub records_emitted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<XrefUnsupportedReason>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub omissions: BTreeMap<XrefOmissionReason, usize>,
}

impl XrefLayerCoverage {
    pub fn not_requested(
        layer: XrefAnalysisLayer,
        scope: impl Into<String>,
        unit: XrefCoverageUnit,
    ) -> Self {
        Self {
            layer,
            status: XrefAnalysisState::NotRequested,
            scope: scope.into(),
            unit,
            candidate_units: 0,
            completed_units: 0,
            omitted_units: 0,
            records_emitted: 0,
            unsupported_reason: None,
            omissions: BTreeMap::new(),
        }
    }

    pub fn requested(
        layer: XrefAnalysisLayer,
        scope: impl Into<String>,
        unit: XrefCoverageUnit,
    ) -> Self {
        Self {
            layer,
            status: XrefAnalysisState::CompleteForScope,
            scope: scope.into(),
            unit,
            candidate_units: 0,
            completed_units: 0,
            omitted_units: 0,
            records_emitted: 0,
            unsupported_reason: None,
            omissions: BTreeMap::new(),
        }
    }

    pub fn mark_unsupported(&mut self, reason: XrefUnsupportedReason) {
        self.status = XrefAnalysisState::Unsupported;
        self.unsupported_reason = Some(reason);
    }

    pub fn omit(&mut self, reason: XrefOmissionReason, count: usize) {
        if count > 0 {
            *self.omissions.entry(reason).or_default() += count;
        }
    }

    pub fn finalize(&mut self) {
        self.omitted_units = self.candidate_units.saturating_sub(self.completed_units);
        if matches!(
            self.status,
            XrefAnalysisState::NotRequested
                | XrefAnalysisState::Unsupported
                | XrefAnalysisState::Failed
        ) {
            return;
        }
        self.status = if self.omissions.is_empty() {
            if self.omitted_units == 0 {
                XrefAnalysisState::CompleteForScope
            } else {
                XrefAnalysisState::Partial
            }
        } else {
            XrefAnalysisState::Partial
        };
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct XrefAnalysisReport {
    pub layers: Vec<XrefLayerCoverage>,
}
