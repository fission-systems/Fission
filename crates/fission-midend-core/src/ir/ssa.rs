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
    pub inputs: BTreeMap<SsaStorageKey, SsaValueId>,
    pub operation_outputs: BTreeMap<SsaOpSite, SsaValueId>,
    pub operation_inputs: BTreeMap<SsaUseSite, SsaValueId>,
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
    InputDefinitionMismatch {
        storage: SsaStorageKey,
        value: SsaValueId,
    },
    OperationDefinitionMismatch {
        site: SsaOpSite,
        value: SsaValueId,
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

        for (site, value_id) in &self.operation_outputs {
            require_value(*value_id)?;
            if self.value(*value_id).expect("validated value").definition
                != SsaValueDefinition::Operation(*site)
            {
                return Err(ScalarSsaShapeError::OperationDefinitionMismatch {
                    site: *site,
                    value: *value_id,
                });
            }
        }

        for value_id in self.operation_inputs.values() {
            require_value(*value_id)?;
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
}
