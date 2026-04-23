use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::providers::ExecutionProvider;
use crate::runtime::{DecodedInstruction, ExecutionProviderKey};

#[path = "../processors/x86/generated.rs"]
mod generated_runtime;

pub(crate) struct X86_64GeneratedProvider;

pub(crate) static X86_64_GENERATED_PROVIDER: X86_64GeneratedProvider = X86_64GeneratedProvider;

impl ExecutionProvider for X86_64GeneratedProvider {
    fn key(&self) -> ExecutionProviderKey {
        ExecutionProviderKey::X86_64Generated
    }

    fn supports(&self, entry: &EntrySpec, _compiled: &CompiledFrontend) -> bool {
        entry.entry_id.eq_ignore_ascii_case("x86-64")
    }

    fn decode_and_lift(
        &self,
        _entry: &EntrySpec,
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        generated_runtime::decode_and_lift(compiled, bytes, address)
    }

    fn decode_instruction(
        &self,
        _entry: &EntrySpec,
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) -> Result<DecodedInstruction> {
        generated_runtime::decode_instruction(compiled, bytes, address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{compile_x86_64_frontend, discover_all_entry_specs};
    use crate::runtime::DecodedFlowKind;

    fn x86_64_entry() -> EntrySpec {
        discover_all_entry_specs()
            .expect("discover entry specs")
            .into_iter()
            .find(|entry| entry.entry_id == "x86-64")
            .expect("x86-64 entry")
    }

    #[test]
    fn x86_64_provider_supports_x86_64_entry() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let entry = x86_64_entry();
        assert!(X86_64_GENERATED_PROVIDER.supports(&entry, &compiled));
    }

    #[test]
    fn x86_64_provider_decodes_ret() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        let entry = x86_64_entry();
        let instruction = X86_64_GENERATED_PROVIDER
            .decode_instruction(&entry, &compiled, &[0xC3], 0x1000)
            .expect("decode ret");
        assert_eq!(instruction.length, 1);
        assert_eq!(instruction.flow_kind, DecodedFlowKind::Return);
    }
}
