//! Typed scalar-SSA facts shared by builder, type recovery, and later
//! out-of-SSA/HighVariable recovery.
//!
//! The first Heritage phase intentionally models exact scalar storage
//! locations only. Overlapping/subregister refinement and memory guards are
//! separate phases because they change which storage locations are eligible
//! for SSA, while these types only describe identities after that decision.

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
