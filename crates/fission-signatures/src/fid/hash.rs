//! Ghidra-style FID hashing primitives.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FidHashQuad {
    pub code_unit_size: u16,
    pub full_hash: u64,
    pub specific_hash_additional_size: u8,
    pub specific_hash: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FidHashError {
    #[error("function has fewer code units than the short hash limit")]
    TooFewCodeUnits,
    #[error("instruction mask/operand metadata is required for exact FID hashing")]
    UnsupportedFidHashInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FidOperandValue {
    Scalar { value: i64, is_address: bool },
    Register { offset: i32 },
    Address,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FidInstructionOperand {
    pub values: Vec<FidOperandValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FidHashUnit {
    pub bytes: Vec<u8>,
    pub instruction_mask: Option<Vec<u8>>,
    pub operands: Vec<FidInstructionOperand>,
    pub is_call: bool,
    pub has_relocation: bool,
}

#[derive(Debug, Clone)]
pub struct FidHasher {
    short_code_unit_limit: u8,
}

impl FidHasher {
    pub fn new(short_code_unit_limit: u8) -> Self {
        Self {
            short_code_unit_limit,
        }
    }

    pub fn hash(&self, units: &[FidHashUnit]) -> Result<FidHashQuad, FidHashError> {
        let mut full_units = 0u16;
        let mut call_count = 0u16;
        let mut specific_count = 0u8;
        let mut full = StableDigest::new();
        let mut specific = StableDigest::new();

        for unit in units {
            if is_x86_nop(unit.bytes.as_slice()) {
                continue;
            }
            let Some(mask) = unit.instruction_mask.as_ref() else {
                return Err(FidHashError::UnsupportedFidHashInput);
            };
            if mask.len() != unit.bytes.len() {
                return Err(FidHashError::UnsupportedFidHashInput);
            }

            full_units = full_units.saturating_add(1);
            if unit.is_call {
                call_count = call_count.saturating_add(1);
            }

            for (operand_idx, operand) in unit.operands.iter().enumerate() {
                let mut specific_update = ((operand_idx as i32) + 1) * 7777;
                let mut full_update = specific_update;
                for value in &operand.values {
                    match value {
                        FidOperandValue::Scalar { value, is_address } => {
                            let mut val = *value;
                            if unit.has_relocation || *is_address || val >= 256 || val <= -256 {
                                val = 0xfeed_dead;
                            } else {
                                specific_count = specific_count.saturating_add(1);
                            }
                            specific_update =
                                specific_update.wrapping_add(((val as i32).wrapping_add(1_234_567)).wrapping_mul(67_999));
                            full_update = full_update.wrapping_add(0xfeed_deadu32 as i32);
                        }
                        FidOperandValue::Register { offset } => {
                            let val = offset.wrapping_add(7_654_321).wrapping_mul(98_777);
                            full_update = full_update.wrapping_add(val);
                            specific_update = specific_update.wrapping_add(val);
                        }
                        FidOperandValue::Address => {
                            specific_update = specific_update
                                .wrapping_add((0xfeed_deadu32 as i32).wrapping_mul(67_999));
                            full_update = full_update.wrapping_add(0xfeed_deadu32 as i32);
                        }
                    }
                }
                full.update_i32(full_update);
                specific.update_i32(specific_update);
            }

            for (byte, mask_byte) in unit.bytes.iter().zip(mask.iter()) {
                full.update_byte(byte & mask_byte);
                specific.update_byte(byte & mask_byte);
            }
        }

        if full_units < self.short_code_unit_limit as u16 {
            return Err(FidHashError::TooFewCodeUnits);
        }

        Ok(FidHashQuad {
            code_unit_size: full_units.saturating_sub(call_count),
            full_hash: full.finish(),
            specific_hash_additional_size: specific_count,
            specific_hash: specific.finish(),
        })
    }
}

#[derive(Debug, Clone)]
struct StableDigest {
    state: u64,
}

impl StableDigest {
    fn new() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn update_byte(&mut self, byte: u8) {
        self.state ^= u64::from(byte);
        self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        self.state ^= self.state >> 32;
    }

    fn update_i32(&mut self, value: i32) {
        for byte in value.to_le_bytes() {
            self.update_byte(byte);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

impl Default for FidHasher {
    fn default() -> Self {
        Self::new(4)
    }
}

pub const X86_NOP_SKIPPER: &[&[u8]] = &[
    &[0x90],
    &[0x8b, 0xc0],
    &[0x8b, 0xc9],
    &[0x8b, 0xd2],
    &[0x8b, 0xdb],
    &[0x8b, 0xe4],
    &[0x8b, 0xed],
    &[0x8b, 0xf6],
    &[0x8b, 0xff],
    &[0x66, 0x90],
    &[0x0f, 0x1f, 0x00],
    &[0x0f, 0x1f, 0x40, 0x00],
    &[0x0f, 0x1f, 0x44, 0x00, 0x00],
    &[0x66, 0x0f, 0x1f, 0x44, 0x00, 0x00],
    &[0x0f, 0x1f, 0x80, 0x00, 0x00, 0x00, 0x00],
    &[0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
    &[0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
];

fn is_x86_nop(bytes: &[u8]) -> bool {
    X86_NOP_SKIPPER.iter().any(|pattern| *pattern == bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_hash_is_deterministic() {
        let hasher = FidHasher::new(1);
        let unit = FidHashUnit {
            bytes: vec![0x48, 0x89, 0xd8],
            instruction_mask: Some(vec![0xff, 0xff, 0xff]),
            operands: vec![FidInstructionOperand {
                values: vec![FidOperandValue::Register { offset: 0 }],
            }],
            is_call: false,
            has_relocation: false,
        };
        let left = hasher.hash(std::slice::from_ref(&unit)).expect("hash");
        let right = hasher.hash(&[unit]).expect("hash");
        assert_eq!(left, right);
    }

    #[test]
    fn unsupported_without_instruction_mask() {
        let hasher = FidHasher::new(1);
        let err = hasher
            .hash(&[FidHashUnit {
                bytes: vec![0x48, 0x89, 0xd8],
                instruction_mask: None,
                operands: Vec::new(),
                is_call: false,
                has_relocation: false,
            }])
            .expect_err("missing mask must fail closed");
        assert_eq!(err, FidHashError::UnsupportedFidHashInput);
    }

    #[test]
    fn x86_nop_is_skipped_from_code_unit_count() {
        let hasher = FidHasher::new(1);
        let err = hasher
            .hash(&[FidHashUnit {
                bytes: vec![0x90],
                instruction_mask: Some(vec![0xff]),
                operands: Vec::new(),
                is_call: false,
                has_relocation: false,
            }])
            .expect_err("nop-only function is too short after skipping");
        assert_eq!(err, FidHashError::TooFewCodeUnits);
    }
}
