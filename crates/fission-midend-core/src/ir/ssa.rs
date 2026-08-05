//! Typed scalar-SSA facts shared by builder, type recovery, and later
//! out-of-SSA/HighVariable recovery.
//!
//! Scalar Heritage models guarded register/unique storage, overlapping byte
//! partitions, typed dynamic memory/call effects, exact stack/RAM promotion,
//! and conservative out-of-SSA/HighVariable congruence.

use super::CallEffectSummarySource;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaStorageKey {
    pub space_id: u64,
    pub offset: u64,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SsaAddressSpaceKind {
    Register,
    Unique,
}

/// Conservative admission record for one address space participating in SSA.
///
/// The bounds describe the observed half-open byte range. Memory-like and
/// special spaces do not receive a guard until their alias/effect model exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaAddressSpaceGuard {
    pub kind: SsaAddressSpaceKind,
    pub observed_start: u64,
    pub observed_end_exclusive: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaOpSite {
    /// Dense CFG block index, not a machine address.
    pub block: u32,
    pub op: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaUseSite {
    /// Dense CFG block index, not a machine address.
    pub block: u32,
    pub op: u32,
    pub input: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaValueDefinition {
    Input,
    Operation(SsaOpSite),
    Phi { block: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaValue {
    pub id: SsaValueId,
    pub storage: SsaStorageKey,
    pub definition: SsaValueDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaPhiOperand {
    pub predecessor: u32,
    pub value: SsaValueId,
}

/// One disjoint SSA value participating in a wider P-code varnode access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaAccessPiece {
    /// Physical byte offset from the start of the original varnode.
    pub byte_offset: u32,
    pub value: SsaValueId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaDynamicGuardKind {
    Load,
    Store,
    Call,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaMemoryEffect {
    None,
    Read,
    Write,
    ReadWrite,
}

impl SsaMemoryEffect {
    pub fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SsaMemoryRegion {
    Stack,
    Ram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaGuardRangePrecision {
    Unknown,
    Exact,
    Bounded,
}

/// Conservative indirect-effect range for a dynamic memory operation or call.
///
/// Offsets are half-open. `maximum_offset_exclusive == None` means that no
/// finite upper bound is proven. Calls use `space_id == None` until a typed
/// call-effect model can enumerate affected address spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaDynamicGuard {
    pub site: SsaOpSite,
    pub kind: SsaDynamicGuardKind,
    pub effect: SsaMemoryEffect,
    pub region: Option<SsaMemoryRegion>,
    pub space_id: Option<u64>,
    pub minimum_offset: i128,
    pub maximum_offset_exclusive: Option<i128>,
    pub step: u64,
    pub precision: SsaGuardRangePrecision,
    /// Resolved direct-call symbol when type facts identify the target.
    pub call_target: Option<String>,
    /// Provenance of the call-effect summary used to narrow the guard.
    pub call_effect_source: Option<CallEffectSummarySource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaMemoryStorageKey {
    pub region: SsaMemoryRegion,
    pub space_id: u64,
    /// Signed for stack-relative locations; absolute RAM offsets remain non-negative.
    pub offset: i128,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaMemoryValueId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaMemoryValue {
    pub id: SsaMemoryValueId,
    pub storage: SsaMemoryStorageKey,
    pub definition: SsaValueDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaMemoryAccessPiece {
    pub byte_offset: u32,
    pub value: SsaMemoryValueId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaMemoryPhiOperand {
    pub predecessor: u32,
    pub value: SsaMemoryValueId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaMemoryPhiNode {
    pub storage: SsaMemoryStorageKey,
    pub output: SsaMemoryValueId,
    pub operands: Vec<SsaMemoryPhiOperand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaHighVariableId(pub u32);

/// Conservative source-variable congruence recovered from scalar SSA.
///
/// Phi congruence is forced. Adjacent COPY/CAST congruence is speculative and
/// admitted only when aggregate value covers do not interfere.
/// `crossing_guards` records indirect effects crossed by a member's live range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaHighVariable {
    pub id: SsaHighVariableId,
    pub members: Vec<SsaValueId>,
    pub storage_family: Vec<SsaStorageKey>,
    pub crossing_guards: Vec<SsaOpSite>,
    /// Block-local half-open live intervals used for interference checks.
    pub cover: Vec<SsaCoverBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaCoverBlock {
    pub block: u32,
    pub start: u32,
    pub end_exclusive: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaMemoryHighVariableId(pub u32);

/// Memory-side analog of `SsaHighVariable`: congruence over `SsaMemoryValueId`s
/// at one exact stack/RAM storage location. Unlike the scalar case, memory
/// values are only forced-congruent via `memory_phis` (there is no memory
/// analog of a scalar `Copy`/`Cast` speculative merge -- a `Store` never reads
/// a value implying congruence with what preceded it at that address).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaMemoryHighVariable {
    pub id: SsaMemoryHighVariableId,
    pub members: Vec<SsaMemoryValueId>,
    /// Block-local half-open live intervals used for interference checks.
    pub cover: Vec<SsaCoverBlock>,
}

/// One parallel-copy requirement for destroying a phi on an incoming edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SsaOutOfSsaCopy {
    pub successor: u32,
    pub predecessor: u32,
    pub storage: SsaStorageKey,
    pub source: SsaValueId,
    pub destination: SsaValueId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirPhiNode {
    pub storage: SsaStorageKey,
    pub output: SsaValueId,
    pub operands: Vec<SsaPhiOperand>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NirScalarSsa {
    /// Dense by `SsaValueId`: `values[id.0 as usize].id == id`.
    pub values: Vec<SsaValue>,
    pub address_spaces: BTreeMap<u64, SsaAddressSpaceGuard>,
    pub inputs: BTreeMap<SsaStorageKey, SsaValueId>,
    pub operation_outputs: BTreeMap<SsaOpSite, Vec<SsaAccessPiece>>,
    pub operation_inputs: BTreeMap<SsaUseSite, Vec<SsaAccessPiece>>,
    /// Phi nodes keyed by their dense CFG block index.
    pub phis: BTreeMap<u32, Vec<NirPhiNode>>,
    pub dynamic_guards: BTreeMap<SsaOpSite, SsaDynamicGuard>,
    /// Promoted exact stack/RAM locations, dense by `SsaMemoryValueId`.
    pub memory_values: Vec<SsaMemoryValue>,
    pub memory_inputs: BTreeMap<SsaMemoryStorageKey, SsaMemoryValueId>,
    pub memory_operation_inputs: BTreeMap<SsaOpSite, Vec<SsaMemoryAccessPiece>>,
    pub memory_operation_outputs: BTreeMap<SsaOpSite, Vec<SsaMemoryAccessPiece>>,
    pub memory_phis: BTreeMap<u32, Vec<SsaMemoryPhiNode>>,
    /// Parallel copies, ordered by successor, predecessor, then storage.
    pub out_of_ssa_copies: Vec<SsaOutOfSsaCopy>,
    /// Dense by `SsaHighVariableId`.
    pub high_variables: Vec<SsaHighVariable>,
    /// Dense by `SsaValueId`.
    pub value_high_variables: Vec<SsaHighVariableId>,
    /// Dense by `SsaMemoryHighVariableId`.
    pub memory_high_variables: Vec<SsaMemoryHighVariable>,
    /// Dense by `SsaMemoryValueId`.
    pub value_memory_high_variables: Vec<SsaMemoryHighVariableId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarSsaShapeError {
    NonDenseValue {
        index: usize,
        value: SsaValueId,
    },
    UnknownValueReference(SsaValueId),
    MissingAddressSpaceGuard {
        storage: SsaStorageKey,
    },
    StorageOutsideGuard {
        storage: SsaStorageKey,
    },
    InputDefinitionMismatch {
        storage: SsaStorageKey,
        value: SsaValueId,
    },
    OperationDefinitionMismatch {
        site: SsaOpSite,
        value: SsaValueId,
    },
    MalformedOperationOutputPieces {
        site: SsaOpSite,
    },
    MalformedOperationInputPieces {
        site: SsaUseSite,
    },
    PhiDefinitionMismatch {
        block: u32,
        value: SsaValueId,
    },
    PhiOutputStorageMismatch {
        block: u32,
        value: SsaValueId,
        expected: SsaStorageKey,
        actual: SsaStorageKey,
    },
    UnsortedPhiStorage {
        block: u32,
    },
    UnsortedPhiPredecessors {
        block: u32,
        output: SsaValueId,
    },
    MalformedDynamicGuard {
        site: SsaOpSite,
    },
    NonDenseMemoryValue {
        index: usize,
        value: SsaMemoryValueId,
    },
    UnknownMemoryValueReference(SsaMemoryValueId),
    MalformedMemorySsa,
    UnsortedOutOfSsaCopies,
    InvalidOutOfSsaCopy {
        copy: SsaOutOfSsaCopy,
    },
    HighVariableMapLength {
        expected: usize,
        actual: usize,
    },
    NonDenseHighVariable {
        index: usize,
        high: SsaHighVariableId,
    },
    MalformedHighVariable {
        high: SsaHighVariableId,
    },
    HighVariableMembershipMismatch {
        value: SsaValueId,
        expected: SsaHighVariableId,
        actual: SsaHighVariableId,
    },
    MissingHighVariableMember {
        value: SsaValueId,
    },
}

impl NirScalarSsa {
    pub fn value(&self, id: SsaValueId) -> Option<&SsaValue> {
        self.values
            .get(id.0 as usize)
            .filter(|value| value.id == id)
    }

    pub fn memory_value(&self, id: SsaMemoryValueId) -> Option<&SsaMemoryValue> {
        self.memory_values
            .get(id.0 as usize)
            .filter(|value| value.id == id)
    }

    pub fn validate_shape(&self) -> Result<(), ScalarSsaShapeError> {
        for (index, value) in self.values.iter().enumerate() {
            if value.id.0 as usize != index {
                return Err(ScalarSsaShapeError::NonDenseValue {
                    index,
                    value: value.id,
                });
            }
            self.validate_storage_guard(value.storage)?;
        }

        let require_value = |id| {
            self.value(id)
                .map(|_| ())
                .ok_or(ScalarSsaShapeError::UnknownValueReference(id))
        };

        for (storage, value_id) in &self.inputs {
            require_value(*value_id)?;
            let value = self.value(*value_id).expect("validated value");
            if value.storage != *storage || value.definition != SsaValueDefinition::Input {
                return Err(ScalarSsaShapeError::InputDefinitionMismatch {
                    storage: *storage,
                    value: *value_id,
                });
            }
        }

        for (site, pieces) in &self.operation_outputs {
            if !self.access_pieces_are_well_formed(pieces) {
                return Err(ScalarSsaShapeError::MalformedOperationOutputPieces { site: *site });
            }
            for piece in pieces {
                require_value(piece.value)?;
                if self.value(piece.value).expect("validated value").definition
                    != SsaValueDefinition::Operation(*site)
                {
                    return Err(ScalarSsaShapeError::OperationDefinitionMismatch {
                        site: *site,
                        value: piece.value,
                    });
                }
            }
        }

        for (site, pieces) in &self.operation_inputs {
            if !self.access_pieces_are_well_formed(pieces) {
                return Err(ScalarSsaShapeError::MalformedOperationInputPieces { site: *site });
            }
            for piece in pieces {
                require_value(piece.value)?;
            }
        }

        for (block, phis) in &self.phis {
            if !phis
                .windows(2)
                .all(|pair| pair[0].storage < pair[1].storage)
            {
                return Err(ScalarSsaShapeError::UnsortedPhiStorage { block: *block });
            }
            for phi in phis {
                require_value(phi.output)?;
                if self.value(phi.output).expect("validated value").definition
                    != (SsaValueDefinition::Phi { block: *block })
                {
                    return Err(ScalarSsaShapeError::PhiDefinitionMismatch {
                        block: *block,
                        value: phi.output,
                    });
                }
                let output_storage = self.value(phi.output).expect("validated value").storage;
                if output_storage != phi.storage {
                    return Err(ScalarSsaShapeError::PhiOutputStorageMismatch {
                        block: *block,
                        value: phi.output,
                        expected: phi.storage,
                        actual: output_storage,
                    });
                }
                if !phi
                    .operands
                    .windows(2)
                    .all(|pair| pair[0].predecessor < pair[1].predecessor)
                {
                    return Err(ScalarSsaShapeError::UnsortedPhiPredecessors {
                        block: *block,
                        output: phi.output,
                    });
                }
                for operand in &phi.operands {
                    require_value(operand.value)?;
                }
            }
        }

        for (site, guard) in &self.dynamic_guards {
            if guard.site != *site || !guard.is_well_formed() {
                return Err(ScalarSsaShapeError::MalformedDynamicGuard { site: *site });
            }
        }

        for (index, value) in self.memory_values.iter().enumerate() {
            if value.id.0 as usize != index {
                return Err(ScalarSsaShapeError::NonDenseMemoryValue {
                    index,
                    value: value.id,
                });
            }
            if value.storage.size == 0
                || (value.storage.region == SsaMemoryRegion::Ram && value.storage.offset < 0)
                || value
                    .storage
                    .offset
                    .checked_add(i128::from(value.storage.size))
                    .is_none()
            {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
        }
        let require_memory_value = |id: SsaMemoryValueId| {
            self.memory_values
                .get(id.0 as usize)
                .filter(|value| value.id == id)
                .ok_or(ScalarSsaShapeError::UnknownMemoryValueReference(id))
        };
        for (storage, value_id) in &self.memory_inputs {
            let value = require_memory_value(*value_id)?;
            if value.storage != *storage || value.definition != SsaValueDefinition::Input {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
        }
        for (site, pieces) in &self.memory_operation_inputs {
            let Some(guard) = self.dynamic_guards.get(site) else {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            };
            if !guard.effect.reads()
                || pieces.is_empty()
                || (guard.kind == SsaDynamicGuardKind::Load
                    && !self.memory_access_pieces_are_well_formed(pieces))
            {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
            for piece in pieces {
                require_memory_value(piece.value)?;
            }
        }
        for (site, pieces) in &self.memory_operation_outputs {
            if pieces.is_empty() {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
            for piece in pieces {
                let value = require_memory_value(piece.value)?;
                if value.definition != SsaValueDefinition::Operation(*site) {
                    return Err(ScalarSsaShapeError::MalformedMemorySsa);
                }
            }
            let Some(guard) = self.dynamic_guards.get(site) else {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            };
            if !guard.effect.writes() {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
        }
        for (block, phis) in &self.memory_phis {
            if !phis
                .windows(2)
                .all(|pair| pair[0].storage < pair[1].storage)
            {
                return Err(ScalarSsaShapeError::MalformedMemorySsa);
            }
            for phi in phis {
                let output = require_memory_value(phi.output)?;
                if output.storage != phi.storage
                    || output.definition != (SsaValueDefinition::Phi { block: *block })
                    || !phi
                        .operands
                        .windows(2)
                        .all(|pair| pair[0].predecessor < pair[1].predecessor)
                {
                    return Err(ScalarSsaShapeError::MalformedMemorySsa);
                }
                for operand in &phi.operands {
                    if require_memory_value(operand.value)?.storage != phi.storage {
                        return Err(ScalarSsaShapeError::MalformedMemorySsa);
                    }
                }
            }
        }

        if !self
            .out_of_ssa_copies
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        {
            return Err(ScalarSsaShapeError::UnsortedOutOfSsaCopies);
        }
        for copy in &self.out_of_ssa_copies {
            require_value(copy.source)?;
            require_value(copy.destination)?;
            if self.value(copy.source).expect("validated value").storage != copy.storage
                || self
                    .value(copy.destination)
                    .expect("validated value")
                    .storage
                    != copy.storage
            {
                return Err(ScalarSsaShapeError::InvalidOutOfSsaCopy { copy: *copy });
            }
        }

        if self.value_high_variables.len() != self.values.len() {
            return Err(ScalarSsaShapeError::HighVariableMapLength {
                expected: self.values.len(),
                actual: self.value_high_variables.len(),
            });
        }
        let mut seen_values = vec![false; self.values.len()];
        for (index, high) in self.high_variables.iter().enumerate() {
            let expected_id = SsaHighVariableId(index as u32);
            if high.id != expected_id {
                return Err(ScalarSsaShapeError::NonDenseHighVariable {
                    index,
                    high: high.id,
                });
            }
            if high.members.is_empty()
                || !high.members.windows(2).all(|pair| pair[0] < pair[1])
                || !high.storage_family.windows(2).all(|pair| pair[0] < pair[1])
                || !high
                    .crossing_guards
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                || !high.cover.windows(2).all(|pair| pair[0] < pair[1])
                || high
                    .cover
                    .iter()
                    .any(|cover| cover.start >= cover.end_exclusive)
                || high
                    .crossing_guards
                    .iter()
                    .any(|site| !self.dynamic_guards.contains_key(site))
            {
                return Err(ScalarSsaShapeError::MalformedHighVariable { high: high.id });
            }
            let mut actual_storage_family = Vec::new();
            for member in &high.members {
                require_value(*member)?;
                let member_index = member.0 as usize;
                if seen_values[member_index] {
                    return Err(ScalarSsaShapeError::MalformedHighVariable { high: high.id });
                }
                seen_values[member_index] = true;
                actual_storage_family.push(self.value(*member).expect("validated value").storage);
                let actual = self.value_high_variables[member_index];
                if actual != high.id {
                    return Err(ScalarSsaShapeError::HighVariableMembershipMismatch {
                        value: *member,
                        expected: high.id,
                        actual,
                    });
                }
            }
            actual_storage_family.sort_unstable();
            actual_storage_family.dedup();
            if actual_storage_family != high.storage_family {
                return Err(ScalarSsaShapeError::MalformedHighVariable { high: high.id });
            }
        }
        if let Some((index, _)) = seen_values.iter().enumerate().find(|(_, seen)| !**seen) {
            return Err(ScalarSsaShapeError::MissingHighVariableMember {
                value: SsaValueId(index as u32),
            });
        }

        Ok(())
    }

    fn validate_storage_guard(&self, storage: SsaStorageKey) -> Result<(), ScalarSsaShapeError> {
        let Some(guard) = self.address_spaces.get(&storage.space_id) else {
            return Err(ScalarSsaShapeError::MissingAddressSpaceGuard { storage });
        };
        let Some(end) = storage.offset.checked_add(u64::from(storage.size)) else {
            return Err(ScalarSsaShapeError::StorageOutsideGuard { storage });
        };
        if storage.size == 0
            || storage.offset < guard.observed_start
            || end > guard.observed_end_exclusive
        {
            return Err(ScalarSsaShapeError::StorageOutsideGuard { storage });
        }
        Ok(())
    }

    fn access_pieces_are_well_formed(&self, pieces: &[SsaAccessPiece]) -> bool {
        let Some(first) = pieces.first() else {
            return false;
        };
        if first.byte_offset != 0 {
            return false;
        }
        pieces.windows(2).all(|pair| {
            let Some(left) = self.value(pair[0].value) else {
                return false;
            };
            let Some(right) = self.value(pair[1].value) else {
                return false;
            };
            let Some(next_byte_offset) = pair[0].byte_offset.checked_add(left.storage.size) else {
                return false;
            };
            let Some(next_storage_offset) = left
                .storage
                .offset
                .checked_add(u64::from(left.storage.size))
            else {
                return false;
            };
            pair[1].byte_offset == next_byte_offset
                && right.storage.space_id == left.storage.space_id
                && right.storage.offset == next_storage_offset
        })
    }

    fn memory_access_pieces_are_well_formed(&self, pieces: &[SsaMemoryAccessPiece]) -> bool {
        let Some(first) = pieces.first() else {
            return false;
        };
        if first.byte_offset != 0 {
            return false;
        }
        pieces.windows(2).all(|pair| {
            let Some(left) = self.memory_values.get(pair[0].value.0 as usize) else {
                return false;
            };
            let Some(right) = self.memory_values.get(pair[1].value.0 as usize) else {
                return false;
            };
            pair[0]
                .byte_offset
                .checked_add(left.storage.size)
                .is_some_and(|offset| offset == pair[1].byte_offset)
                && left.storage.region == right.storage.region
                && left.storage.space_id == right.storage.space_id
                && left
                    .storage
                    .offset
                    .checked_add(i128::from(left.storage.size))
                    .is_some_and(|offset| offset == right.storage.offset)
        })
    }
}

impl SsaDynamicGuard {
    fn is_well_formed(&self) -> bool {
        if (self.kind == SsaDynamicGuardKind::Load && self.effect != SsaMemoryEffect::Read)
            || (self.kind == SsaDynamicGuardKind::Store && self.effect != SsaMemoryEffect::Write)
        {
            return false;
        }
        if self.kind == SsaDynamicGuardKind::Call
            && (self.region.is_some()
                || self.space_id.is_some()
                || self.precision != SsaGuardRangePrecision::Unknown)
        {
            return false;
        }
        if self.kind != SsaDynamicGuardKind::Call
            && (self.call_target.is_some() || self.call_effect_source.is_some())
        {
            return false;
        }
        match self.precision {
            SsaGuardRangePrecision::Unknown => {
                self.minimum_offset == 0
                    && self.maximum_offset_exclusive.is_none()
                    && self.step == 0
            }
            SsaGuardRangePrecision::Exact => {
                self.maximum_offset_exclusive
                    .is_some_and(|maximum| maximum > self.minimum_offset)
                    && self.step == 0
                    && self.space_id.is_some()
                    && self.region.is_some()
            }
            SsaGuardRangePrecision::Bounded => {
                self.maximum_offset_exclusive
                    .is_some_and(|maximum| maximum > self.minimum_offset)
                    && self.space_id.is_some()
                    && self.region.is_some()
                    && self.maximum_offset_exclusive.is_some_and(|maximum| {
                        self.step == 0 || i128::from(self.step) <= maximum - self.minimum_offset
                    })
            }
        }
    }
}
