//! Typed scalar-SSA facts shared by builder, type recovery, and later
//! out-of-SSA/HighVariable recovery.
//!
//! Scalar Heritage models guarded register/unique storage, overlapping byte
//! partitions, dynamic memory/call effects, and the first conservative
//! out-of-SSA/HighVariable congruence. Memory guards remain facts rather than
//! promoted scalar values until a stack/object alias model owns that decision.

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
    Read,
    Write,
    ReadWrite,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsaDynamicGuard {
    pub site: SsaOpSite,
    pub kind: SsaDynamicGuardKind,
    pub effect: SsaMemoryEffect,
    pub space_id: Option<u64>,
    pub minimum_offset: u64,
    pub maximum_offset_exclusive: Option<u64>,
    pub step: u64,
    pub precision: SsaGuardRangePrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SsaHighVariableId(pub u32);

/// Conservative source-variable congruence recovered from scalar SSA.
///
/// The first phase performs forced phi congruence only. `crossing_guards`
/// records indirect effects crossed by a member's live range so later
/// speculative coalescing can fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaHighVariable {
    pub id: SsaHighVariableId,
    pub members: Vec<SsaValueId>,
    pub storage_family: Vec<SsaStorageKey>,
    pub crossing_guards: Vec<SsaOpSite>,
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
    /// Parallel copies, ordered by successor, predecessor, then storage.
    pub out_of_ssa_copies: Vec<SsaOutOfSsaCopy>,
    /// Dense by `SsaHighVariableId`.
    pub high_variables: Vec<SsaHighVariable>,
    /// Dense by `SsaValueId`.
    pub value_high_variables: Vec<SsaHighVariableId>,
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
}

impl SsaDynamicGuard {
    fn is_well_formed(self) -> bool {
        let expected_effect = match self.kind {
            SsaDynamicGuardKind::Load => SsaMemoryEffect::Read,
            SsaDynamicGuardKind::Store => SsaMemoryEffect::Write,
            SsaDynamicGuardKind::Call => SsaMemoryEffect::ReadWrite,
        };
        if self.effect != expected_effect {
            return false;
        }
        if self.kind == SsaDynamicGuardKind::Call
            && (self.space_id.is_some() || self.precision != SsaGuardRangePrecision::Unknown)
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
            }
            SsaGuardRangePrecision::Bounded => {
                self.maximum_offset_exclusive
                    .is_some_and(|maximum| maximum > self.minimum_offset)
                    && self.space_id.is_some()
                    && self.maximum_offset_exclusive.is_some_and(|maximum| {
                        self.step == 0 || self.step <= maximum - self.minimum_offset
                    })
            }
        }
    }
}
