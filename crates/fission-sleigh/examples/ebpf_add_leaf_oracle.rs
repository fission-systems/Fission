//! Research oracle for the leaf migrated to FSL. No FSL runtime dependency.
//! Verifies the checked-in SLA's register binding and IntAdd effect shape.
use anyhow::{ensure, Result};
use fission_pcode::PcodeOpcode;
use fission_sleigh::compiler::CompiledTemplateSource;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn main() -> Result<()> {
    let frontend = RuntimeSleighFrontend::new_for_language("eBPF:LE:64:default")?;
    let mut count = 0;
    for destination in 0..11u8 {
        for source in 0..11u8 {
            let raw = [
                0x0f,
                destination | source << 4,
                0x34,
                0x12,
                0x78,
                0x56,
                0x34,
                0x12,
            ];
            let (instruction, ops, length, details) =
                frontend.decode_instruction_and_lift_with_context_override(&raw, 0, None)?;
            ensure!(
                length == 8 && instruction.bytes == raw,
                "unexpected decode length/bytes"
            );
            ensure!(
                details.template_source == Some(CompiledTemplateSource::SpecDerived),
                "not SLA derived"
            );
            ensure!(
                ops.len() == 1 && ops[0].opcode == PcodeOpcode::IntAdd,
                "unexpected SLA effect: {ops:?}"
            );
            let op = &ops[0];
            ensure!(op.inputs.len() == 2, "expected two inputs");
            let output = op
                .output
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing destination"))?;
            ensure!(
                output.size == 8 && output.offset == u64::from(destination) * 8,
                "wrong destination binding"
            );
            ensure!(
                op.inputs[0].size == 8 && op.inputs[0].offset == u64::from(destination) * 8,
                "wrong lhs binding"
            );
            ensure!(
                op.inputs[1].size == 8 && op.inputs[1].offset == u64::from(source) * 8,
                "wrong rhs binding"
            );
            ensure!(
                output.space_id == op.inputs[0].space_id
                    && output.space_id == op.inputs[1].space_id,
                "mixed spaces"
            );
            ensure!(
                !output.is_constant && !op.inputs[0].is_constant && !op.inputs[1].is_constant,
                "unexpected constant binding"
            );
            count += 1;
        }
    }
    println!("SLA eBPF ADD64 register leaf: {count} decode/binding/effect comparisons passed");
    Ok(())
}
