//! x86 processor adapter for the shared SLEIGH runtime spine.
//!
//! This module may own x86 field extraction and register/address-space mapping.
//! It must not become a mnemonic-by-mnemonic semantic owner.

use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::DecodedInstruction;

pub mod generated;

pub const SKELETON: super::ProcessorSkeleton = super::ProcessorSkeleton {
    ghidra_processor: "x86",
    module_name: "x86",
    executable_candidate: true,
};

pub(crate) fn supports_entry(entry: &EntrySpec) -> bool {
    entry.entry_id.eq_ignore_ascii_case("x86-64")
}

pub(crate) fn decode_and_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    generated::decode_and_lift(compiled, bytes, address)
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    generated::decode_instruction(compiled, bytes, address)
}
