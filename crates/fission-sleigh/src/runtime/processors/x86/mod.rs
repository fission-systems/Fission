//! x86 processor adapter for the shared SLEIGH runtime spine.
//!
//! This module may own x86 field extraction and register/address-space mapping.
//! It must not become a mnemonic-by-mnemonic semantic owner.

pub mod generated;

pub const SKELETON: super::ProcessorSkeleton = super::ProcessorSkeleton {
    ghidra_processor: "x86",
    module_name: "x86",
    executable_candidate: true,
};
