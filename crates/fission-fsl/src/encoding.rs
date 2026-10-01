//! Verified fixed-width decode plans. These describe bytes, not a second IR.

use std::collections::HashSet;

use crate::{ByteOrder, FslError, FslcPackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitField {
    pub name: String,
    pub offset: u16,
    pub bits: u16,
    /// Encodings requiring an unsupported extension or operand interpretation.
    pub excluded: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoding {
    pub bits: u16,
    pub mask: u128,
    pub value: u128,
    pub fields: Vec<BitField>,
}

impl Encoding {
    pub fn byte_opcode(opcode: u8) -> Self {
        Self {
            bits: 8,
            mask: 255,
            value: u128::from(opcode),
            fields: Vec::new(),
        }
    }

    pub fn opcode(&self) -> Option<u8> {
        (self.bits == 8 && self.mask == 255 && self.value <= 255 && self.fields.is_empty())
            .then_some(self.value as u8)
    }

    pub fn validate(&self) -> Result<(), FslError> {
        if !matches!(self.bits, 8 | 32 | 64 | 128)
            || self.mask == 0
            || self.mask & !mask(self.bits) != 0
            || self.value & !self.mask != 0
        {
            return Err(error("invalid fixed-width encoding mask/value"));
        }
        if self.fields.len() > 256 {
            return Err(error("too many encoding fields"));
        }
        let mut names = HashSet::new();
        let mut occupied = 0;
        for field in &self.fields {
            if field.name.is_empty()
                || field.name.len() > 65535
                || !names.insert(&field.name)
                || !field
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || field.name.as_bytes()[0].is_ascii_digit()
                || field.bits == 0
                || field.bits > 64
                || u32::from(field.offset) + u32::from(field.bits) > u32::from(self.bits)
            {
                return Err(error("invalid or duplicate encoding field"));
            }
            let field_mask = mask(field.bits) << field.offset;
            if occupied & field_mask != 0 {
                return Err(error("encoding fields overlap"));
            }
            occupied |= field_mask;
            let mut excluded = HashSet::new();
            if field.excluded.len() > 256 {
                return Err(error("too many excluded field values"));
            }
            for &value in &field.excluded {
                if u128::from(value) > mask(field.bits) || !excluded.insert(value) {
                    return Err(error("invalid or duplicate excluded field value"));
                }
            }
            let fixed_mask = (self.mask >> field.offset) & mask(field.bits);
            let fixed_value = (self.value >> field.offset) & mask(field.bits);
            let free_bits = field.bits - fixed_mask.count_ones() as u16;
            let excluded_candidates = field
                .excluded
                .iter()
                .filter(|&&v| u128::from(v) & fixed_mask == fixed_value)
                .count();
            if free_bits <= 8 && excluded_candidates == (1usize << free_bits) {
                return Err(error("field exclusions make the encoding unreachable"));
            }
        }
        Ok(())
    }

    pub(crate) fn field_values(&self, word: u128) -> Vec<(String, u64)> {
        self.fields
            .iter()
            .map(|f| (f.name.clone(), ((word >> f.offset) & mask(f.bits)) as u64))
            .collect()
    }

    fn admits(&self, word: u128) -> bool {
        word & self.mask == self.value
            && self.fields.iter().all(|f| {
                !f.excluded
                    .contains(&(((word >> f.offset) & mask(f.bits)) as u64))
            })
    }
}

/// Lossless encoding observation, distinct from executable semantic support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedInstruction {
    pub language: String,
    pub instruction_index: usize,
    pub raw: Vec<u8>,
    pub fields: Vec<(String, u64)>,
}

impl FslcPackage {
    /// Decode one fixed-width instruction with an explicit profile identity.
    /// `None` includes rejected operand selectors; no stream length is guessed.
    pub fn decode_bytes(
        &self,
        language: &str,
        bytes: &[u8],
    ) -> Result<Option<DecodedInstruction>, FslError> {
        self.validate()?;
        if language != self.language {
            return Err(error("input architecture does not match FSL profile"));
        }
        let length = usize::from(self.instructions[0].encoding.bits / 8);
        if bytes.len() < length {
            return Err(error("truncated instruction encoding"));
        }
        let raw = &bytes[..length];
        let word = read_word(raw, self.byte_order);
        Ok(self
            .instructions
            .iter()
            .position(|i| i.encoding.admits(word))
            .map(|instruction_index| DecodedInstruction {
                language: self.language.clone(),
                instruction_index,
                raw: raw.to_vec(),
                fields: self.instructions[instruction_index]
                    .encoding
                    .field_values(word),
            }))
    }

    /// Re-encode a validated observation, preserving every unedited bit.
    /// Fixed bits, excluded selectors, duplicate edits, and foreign observations
    /// are rejected before returning bytes. This is not behavioral recompilation.
    pub fn reencode(
        &self,
        decoded: &DecodedInstruction,
        edits: &[(&str, u64)],
    ) -> Result<Vec<u8>, FslError> {
        let observed = self
            .decode_bytes(&decoded.language, &decoded.raw)?
            .ok_or_else(|| error("observation has no supported encoding"))?;
        if &observed != decoded {
            return Err(error("observation does not match package or raw bytes"));
        }
        let encoding = &self.instructions[decoded.instruction_index].encoding;
        let mut word = read_word(&decoded.raw, self.byte_order);
        let mut names = HashSet::new();
        for &(name, value) in edits {
            if !names.insert(name) {
                return Err(error("duplicate field edit"));
            }
            let field = encoding
                .fields
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(|| error("unknown encoding field"))?;
            if u128::from(value) > mask(field.bits) {
                return Err(error("field edit exceeds declared width"));
            }
            let field_mask = mask(field.bits) << field.offset;
            word = (word & !field_mask) | (u128::from(value) << field.offset);
        }
        if !encoding.admits(word) {
            return Err(error(
                "field edits violate fixed bits or supported selectors",
            ));
        }
        let mut raw = decoded.raw.clone();
        for index in 0..raw.len() {
            let shift = match self.byte_order {
                ByteOrder::Little => index * 8,
                ByteOrder::Big => (raw.len() - 1 - index) * 8,
            };
            raw[index] = (word >> shift) as u8;
        }
        Ok(raw)
    }
}

fn read_word(bytes: &[u8], order: ByteOrder) -> u128 {
    match order {
        ByteOrder::Little => bytes
            .iter()
            .enumerate()
            .fold(0, |v, (i, b)| v | (u128::from(*b) << (i * 8))),
        ByteOrder::Big => bytes.iter().fold(0, |v, b| (v << 8) | u128::from(*b)),
    }
}

pub(crate) fn mask(bits: u16) -> u128 {
    if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    }
}

fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}
