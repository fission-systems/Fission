use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fission_pcode::PcodeOp;
use sleigh_rs::execution::{Execution, Statement};
use sleigh_rs::pattern::{BitConstraint, Pattern};
use sleigh_rs::table::{Constructor, Matcher, VariantId};
use sleigh_rs::Sleigh;

use crate::converter::IRConverter;

pub struct SleighLifter {
    sleigh_context: Sleigh,
    default_context_values: Vec<(sleigh_rs::ContextId, u64)>,
}

pub struct DecodeState<'a> {
    pub bytes: &'a [u8],
    pub address: u64,
    context_bits: Vec<u8>,
}

#[derive(Default)]
struct MatchFailureStats {
    total: usize,
    len_failed: usize,
    token_failed: usize,
    context_failed: usize,
}

impl<'a> DecodeState<'a> {
    pub fn new(bytes: &'a [u8], address: u64, context_bits_len: usize) -> Self {
        let context_bytes_len = context_bits_len.div_ceil(8);
        Self {
            bytes,
            address,
            context_bits: vec![0; context_bytes_len],
        }
    }

    fn instruction_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.bytes.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }

    fn context_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.context_bits.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }

    fn set_context_bit(&mut self, bit_index: usize, value: bool) -> Result<()> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let context_len = self.context_bits.len();
        let byte = self.context_bits.get_mut(byte_index).ok_or_else(|| {
            anyhow::anyhow!(
                "Context bit index out of range: bit_index={}, byte_index={}, len={} bytes",
                bit_index,
                byte_index,
                context_len
            )
        })?;
        if value {
            *byte |= 1u8 << bit_in_byte;
        } else {
            *byte &= !(1u8 << bit_in_byte);
        }
        Ok(())
    }
}

impl SleighLifter {
    pub fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs/languages")
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::spec_dir().join(format!("{}.slaspec", language_name))
    }

    pub fn new_for_language(language_name: &str) -> Result<Self> {
        let spec_path = Self::spec_path_for(language_name);
        Self::new(&spec_path).with_context(|| {
            format!(
                "Failed to load Sleigh spec for language '{}' at {}",
                language_name,
                spec_path.display()
            )
        })
    }

    pub fn new(spec_path: &Path) -> Result<Self> {
        let sleigh_context = sleigh_rs::file_to_sleigh(spec_path)
            .map_err(|e| anyhow::anyhow!("Failed to parse Sleigh specs: {:?}", e))?;
        let default_context_values =
            Self::load_default_context_values(&sleigh_context, spec_path)?;

        Ok(Self {
            sleigh_context,
            default_context_values,
        })
    }

    /// Primary decoding function corresponding to Ghidra's Sleigh engine decoding.
    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    /// Decode one instruction and return both generated pcode and decoded byte length.
    pub fn decode_and_lift_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        let context_bits_len = usize::try_from(self.sleigh_context.context_memory().memory_bits)
            .context("Context bit length does not fit usize")?;
        let mut state = DecodeState::new(bytes, address, context_bits_len);
        self.seed_default_context(&mut state)?;
        let inst_table_id = self.sleigh_context.instruction_table();

        let inst_table = self.sleigh_context.table(inst_table_id);

        // matcher_order() is pre-sorted to follow Sleigh constructor matching priority.
        let matchers = inst_table.matcher_order();
        let mut matched_constructor = None;

        for matcher in matchers {
            let constructor = inst_table.constructor(matcher.constructor);
            let is_match = self
                .match_pattern(&state, constructor, matcher.variant_id)
                .with_context(|| {
                    format!(
                        "Failed to evaluate constructor pattern at address {:#x}",
                        address
                    )
                })?;

            if is_match {
                matched_constructor = Some(constructor);
                break;
            }
        }

        if let Some(constructor) = matched_constructor {
            // The caller probes increasing byte windows and stops at the first
            // successful match. Advancing by that probe window keeps the
            // decode cursor aligned with the matched variant.
            let decoded_len = u64::try_from(bytes.len())
                .context("Decoded instruction length does not fit u64")?;
            let delay_slot_bytes = constructor
                .execution
                .as_ref()
                .map(Self::execution_delay_slot_bytes)
                .unwrap_or(0);
            let (next_address, next2_address) =
                Self::compute_next_addresses(address, decoded_len, delay_slot_bytes)?;
            let mut ops = Vec::new();
            if let Some(exec) = &constructor.execution {
                let mut converter = IRConverter::new_with_decode_state(
                    state.bytes,
                    &state.context_bits,
                );
                let converted_ops = converter
                    .convert_execution(
                        exec,
                        address,
                        next_address,
                        next2_address,
                        &self.sleigh_context,
                    )
                    .with_context(|| {
                        format!("Failed to convert execution at address {:#x}", address)
                    })?;
                ops.extend(converted_ops);
            }
            Ok((ops, decoded_len))
        } else {
            let stats = self.collect_match_failure_stats(&state, matchers);
            anyhow::bail!(
                "No matching Sleigh constructor found at address {:#x} (total={}, len_failed={}, token_failed={}, context_failed={}, probe_bytes={})",
                address,
                stats.total,
                stats.len_failed,
                stats.token_failed,
                stats.context_failed,
                bytes.len()
            )
        }
    }

    fn match_pattern(
        &self,
        state: &DecodeState,
        constructor: &Constructor,
        variant_id: VariantId,
    ) -> Result<bool> {
        if !self.pattern_len_matches(state, &constructor.pattern) {
            return Ok(false);
        }

        let (context_constraints, token_constraints) = constructor.variant(variant_id);
        self.match_variant_constraints(state, context_constraints, token_constraints)
    }

    fn pspec_path_for_spec(spec_path: &Path) -> PathBuf {
        spec_path.with_extension("pspec")
    }

    fn extract_xml_attr<'a>(line: &'a str, attr: &str) -> Option<&'a str> {
        let needle = format!(r#"{}=""#, attr);
        let start = line.find(&needle)? + needle.len();
        let end = line[start..].find('"')? + start;
        Some(&line[start..end])
    }

    fn parse_pspec_context_set(pspec_path: &Path) -> Result<HashMap<String, u64>> {
        let xml = std::fs::read_to_string(pspec_path).with_context(|| {
            format!(
                "Failed to read pspec context defaults from {}",
                pspec_path.display()
            )
        })?;

        let mut in_context_set = false;
        let mut out = HashMap::new();

        for raw_line in xml.lines() {
            let line = raw_line.trim();
            if !in_context_set {
                if line.starts_with("<context_set") {
                    in_context_set = true;
                }
                continue;
            }

            if line.starts_with("</context_set") {
                break;
            }
            if !line.contains("<set") {
                continue;
            }

            let Some(name) = Self::extract_xml_attr(line, "name") else {
                continue;
            };
            let Some(val_raw) = Self::extract_xml_attr(line, "val") else {
                continue;
            };

            let value = if let Some(hex) =
                val_raw.strip_prefix("0x").or_else(|| val_raw.strip_prefix("0X"))
            {
                u64::from_str_radix(hex, 16).with_context(|| {
                    format!(
                        "Invalid hex context value '{}' for '{}' in {}",
                        val_raw,
                        name,
                        pspec_path.display()
                    )
                })?
            } else {
                val_raw.parse::<u64>().with_context(|| {
                    format!(
                        "Invalid decimal context value '{}' for '{}' in {}",
                        val_raw,
                        name,
                        pspec_path.display()
                    )
                })?
            };

            out.insert(name.to_string(), value);
        }

        Ok(out)
    }

    fn load_default_context_values(
        sleigh_context: &Sleigh,
        spec_path: &Path,
    ) -> Result<Vec<(sleigh_rs::ContextId, u64)>> {
        let pspec_path = Self::pspec_path_for_spec(spec_path);
        if !pspec_path.exists() {
            return Ok(Vec::new());
        }

        let defaults_by_name = Self::parse_pspec_context_set(&pspec_path)?;
        if defaults_by_name.is_empty() {
            return Ok(Vec::new());
        }

        let mut defaults = Vec::new();
        for (idx, context) in sleigh_context.contexts().iter().enumerate() {
            if let Some(value) = defaults_by_name.get(context.name()) {
                defaults.push((sleigh_rs::ContextId(idx), *value));
            }
        }
        Ok(defaults)
    }

    fn context_storage_bit_for_value_bit(field_len: usize, value_bit: usize) -> usize {
        (field_len - 1) - value_bit
    }

    fn write_context_value(
        &self,
        state: &mut DecodeState,
        context_id: sleigh_rs::ContextId,
        value: u64,
    ) -> Result<()> {
        let context = self.sleigh_context.context(context_id);
        let field_len = usize::try_from(context.bitrange.bits.len().get())
            .context("Context field length does not fit usize")?;
        if field_len > 64 {
            anyhow::bail!(
                "Context field '{}' width {} exceeds 64 bits",
                context.name(),
                field_len
            );
        }

        let mapped_bits = self.sleigh_context.context_memory().context(context_id);
        let mapped_start = usize::try_from(mapped_bits.start())
            .context("Mapped context bit start does not fit usize")?;

        let masked_value = if field_len == 64 {
            value
        } else {
            value & ((1u64 << field_len) - 1)
        };

        for value_bit in 0..field_len {
            let bit_set = ((masked_value >> value_bit) & 1) != 0;
            let field_bit = Self::context_storage_bit_for_value_bit(field_len, value_bit);
            state.set_context_bit(mapped_start + field_bit, bit_set)?;
        }

        Ok(())
    }

    fn seed_default_context(&self, state: &mut DecodeState) -> Result<()> {
        for (context_id, value) in &self.default_context_values {
            self.write_context_value(state, *context_id, *value)?;
        }
        Ok(())
    }

    fn pattern_len_matches(&self, state: &DecodeState, pattern: &Pattern) -> bool {
        let available_bytes = state.bytes.len() as u64;
        if available_bytes < pattern.len.min() {
            return false;
        }
        true
    }

    fn match_variant_constraints(
        &self,
        state: &DecodeState,
        context_constraints: &[BitConstraint],
        token_constraints: &[BitConstraint],
    ) -> Result<bool> {
        if !self.token_constraints_match(state, token_constraints) {
            return Ok(false);
        }
        if !self.context_constraints_match(state, context_constraints) {
            return Ok(false);
        }

        Ok(true)
    }

    fn token_constraints_match(
        &self,
        state: &DecodeState,
        token_constraints: &[BitConstraint],
    ) -> bool {
        for (bit_index, constraint) in token_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let Some(actual) = state.instruction_bit(bit_index) else {
                        return false;
                    };
                    if actual != *expected {
                        return false;
                    }
                }
                BitConstraint::Restrained => {
                    return false;
                }
            }
        }

        true
    }

    fn context_constraints_match(
        &self,
        state: &DecodeState,
        context_constraints: &[BitConstraint],
    ) -> bool {
        for (bit_index, constraint) in context_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let actual = state.context_bit(bit_index).unwrap_or(false);
                    if actual != *expected {
                        return false;
                    }
                }
                BitConstraint::Restrained => {
                    return false;
                }
            }
        }

        true
    }

    fn collect_match_failure_stats(
        &self,
        state: &DecodeState,
        matchers: &[Matcher],
    ) -> MatchFailureStats {
        let inst_table = self.sleigh_context.table(self.sleigh_context.instruction_table());
        let mut stats = MatchFailureStats {
            total: matchers.len(),
            ..MatchFailureStats::default()
        };

        for matcher in matchers {
            let constructor = inst_table.constructor(matcher.constructor);
            if !self.pattern_len_matches(state, &constructor.pattern) {
                stats.len_failed += 1;
                continue;
            }

            let (context_constraints, token_constraints) =
                constructor.variant(matcher.variant_id);
            if !self.token_constraints_match(state, token_constraints) {
                stats.token_failed += 1;
                continue;
            }

            if !self.context_constraints_match(state, context_constraints) {
                stats.context_failed += 1;
            }
        }

        stats
    }

    fn checked_advance_address(address: u64, len_bytes: u64) -> Result<u64> {
        address
            .checked_add(len_bytes)
            .context("Instruction next address overflow")
    }

    fn compute_next_addresses(
        address: u64,
        decoded_len: u64,
        delay_slot_bytes: u64,
    ) -> Result<(u64, u64)> {
        let next_address = Self::checked_advance_address(address, decoded_len)?;
        let next2_address =
            Self::checked_advance_address(next_address, delay_slot_bytes)?;
        Ok((next_address, next2_address))
    }

    fn execution_delay_slot_bytes(execution: &Execution) -> u64 {
        execution
            .blocks()
            .iter()
            .flat_map(|block| block.statements.iter())
            .filter_map(|stmt| match stmt {
                Statement::Delayslot(bytes) => Some(*bytes),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodeState, SleighLifter};

    #[test]
    fn instruction_bit_reads_lsb_first() {
        let state = DecodeState::new(&[0b0000_0101], 0, 0);
        assert_eq!(state.instruction_bit(0), Some(true));
        assert_eq!(state.instruction_bit(1), Some(false));
        assert_eq!(state.instruction_bit(2), Some(true));
    }

    #[test]
    fn out_of_range_instruction_bit_is_none() {
        let state = DecodeState::new(&[0x00], 0, 0);
        assert_eq!(state.instruction_bit(8), None);
    }

    #[test]
    fn set_context_bit_writes_value() {
        let mut state = DecodeState::new(&[0x00], 0, 8);
        state.set_context_bit(3, true).unwrap();
        assert_eq!(state.context_bit(3), Some(true));
        state.set_context_bit(3, false).unwrap();
        assert_eq!(state.context_bit(3), Some(false));
    }

    #[test]
    fn context_storage_bit_for_value_bit_is_msb_first() {
        assert_eq!(SleighLifter::context_storage_bit_for_value_bit(2, 0), 1);
        assert_eq!(SleighLifter::context_storage_bit_for_value_bit(2, 1), 0);
    }

    #[test]
    fn extract_xml_attr_reads_quoted_attr() {
        let line = r#"<set name="opsize" val="1"/>"#;
        assert_eq!(SleighLifter::extract_xml_attr(line, "name"), Some("opsize"));
        assert_eq!(SleighLifter::extract_xml_attr(line, "val"), Some("1"));
    }

    #[test]
    fn checked_advance_address_adds_length() {
        let next = SleighLifter::checked_advance_address(0x1000, 4).unwrap();
        assert_eq!(next, 0x1004);
    }

    #[test]
    fn checked_advance_address_overflow_is_error() {
        let err = SleighLifter::checked_advance_address(u64::MAX, 1);
        assert!(err.is_err());
    }

    #[test]
    fn compute_next_addresses_applies_delay_slot_bytes() {
        let (next, next2) =
            SleighLifter::compute_next_addresses(0x1000, 4, 8).unwrap();
        assert_eq!(next, 0x1004);
        assert_eq!(next2, 0x100c);
    }

    #[test]
    fn compute_next_addresses_overflow_on_next2_is_error() {
        let err = SleighLifter::compute_next_addresses(u64::MAX - 2, 2, 1);
        assert!(err.is_err());
    }

    #[test]
    fn spec_path_for_uses_fission_sleigh_specs_dir() {
        let path = SleighLifter::spec_path_for("x86-64");
        assert!(path.ends_with("specs/languages/x86-64.slaspec"));
    }

    #[test]
    fn aarch64_apple_silicon_spec_parses() {
        let lifter = SleighLifter::new_for_language("AARCH64_AppleSilicon");
        assert!(
            lifter.is_ok(),
            "AARCH64_AppleSilicon must parse successfully: {:#}",
            lifter.err().unwrap()
        );
    }
}
