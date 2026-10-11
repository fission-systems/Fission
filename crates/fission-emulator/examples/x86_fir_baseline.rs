//! Research-only P-code baseline using existing SLA frontend and evaluator.
//! No dependency from the owned FSL runtime to this example.
use anyhow::{Context, Result, ensure};
use fission_emulator::{
    Evaluator, MachineState,
    pcode::{eval::StepResult, spaces::SpaceLayout},
};
use fission_loader::LoadedBinary;
use fission_sleigh::{
    compiler::CompiledTemplateSource,
    runtime::{RuntimeSleighFrontend, register_map_for_load_spec},
};
use std::io::{self, BufRead};

fn read_word(state: &mut MachineState, location: &(u64, u64, u32)) -> Result<u64> {
    let mut bytes = [0u8; 8];
    ensure!(location.2 <= 8, "baseline wide register unsupported");
    state.read_into(location.0, location.1, &mut bytes[..location.2 as usize])?;
    Ok(u64::from_le_bytes(bytes))
}
fn write_word(state: &mut MachineState, location: &(u64, u64, u32), value: u64) -> Result<()> {
    ensure!(location.2 <= 8, "baseline wide register unsupported");
    state.write_space(
        location.0,
        location.1,
        &value.to_le_bytes()[..location.2 as usize],
    )
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "usage: x86_fir_baseline ELF ADDRESS_HEX SIZE; JSON cases on stdin"
    );
    let binary = LoadedBinary::from_file(std::path::Path::new(&args[0]))?;
    let spec = binary.load_spec().context("no canonical load spec")?;
    let mode = binary
        .architecture
        .as_ref()
        .context("no architecture")?
        .bitness;
    ensure!(mode == 32 || mode == 64, "baseline admits x86 32/64 only");
    let address = u64::from_str_radix(args[1].trim_start_matches("0x"), 16)?;
    let size: usize = args[2].parse()?;
    let raw = binary
        .view_executable_bytes(address, size)
        .context("not executable bytes")?;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(spec)?;
    let registers = register_map_for_load_spec(spec).context("no SLA register map")?;
    let spaces = SpaceLayout::from_compiled(
        frontend
            .compiled_frontend()
            .context("no compiled frontend")?,
    );
    let gpr64 = [
        "RAX", "RCX", "RDX", "RBX", "RSP", "RBP", "RSI", "RDI", "R8", "R9", "R10", "R11", "R12",
        "R13", "R14", "R15",
    ];
    let gpr32 = ["EAX", "ECX", "EDX", "EBX", "ESP", "EBP", "ESI", "EDI"];
    let names: &[&str] = if mode == 64 { &gpr64 } else { &gpr32 };
    let flags = ["CF", "PF", "ZF", "SF", "OF", "AF"];
    let mut instructions = Vec::new();
    let mut offset = 0;
    while offset < raw.len() {
        let (_, ops, length, details) = frontend
            .decode_instruction_and_lift_with_context_override(
                &raw[offset..],
                address + offset as u64,
                None,
            )?;
        ensure!(
            length > 0 && offset + length as usize <= raw.len(),
            "bad baseline length"
        );
        ensure!(
            details.template_source == Some(CompiledTemplateSource::SpecDerived),
            "baseline requires actual SLA templates"
        );
        instructions.push(ops);
        offset += length as usize;
    }
    for line in io::stdin().lock().lines() {
        let input: serde_json::Value = serde_json::from_str(&line?)?;
        let input_registers = input["registers"].as_array().context("register array")?;
        let input_flags = input["flags"].as_array().context("flag array")?;
        ensure!(
            input_registers.len() == 17 && input_flags.len() == 12,
            "bank contract"
        );
        let memory = hex::decode(input["memory_hex"].as_str().context("memory hex")?)?;
        let base = input["memory_base"].as_u64().context("memory base")?;
        let mut state = MachineState::with_layout(spaces.clone());
        state.write_space(spaces.ram, base, &memory)?;
        for (slot, name) in names.iter().enumerate() {
            write_word(
                &mut state,
                registers.get(*name).context("missing GPR")?,
                input_registers[slot].as_u64().context("GPR value")?,
            )?;
        }
        for (slot, name) in flags.iter().enumerate() {
            write_word(
                &mut state,
                registers.get(*name).context("missing flag")?,
                input_flags[slot].as_u64().context("flag value")?,
            )?;
        }
        let mut solver = fission_solver::Solver::new();
        let mut exit = None;
        for ops in &instructions {
            let mut evaluator = Evaluator::new(&mut state, &mut solver);
            for op in ops {
                let result = evaluator.step(op)?;
                ensure!(
                    evaluator.unimplemented.is_none(),
                    "unimplemented P-code baseline opcode"
                );
                match result {
                    StepResult::Next => (),
                    StepResult::Branch(target)
                        if op.opcode == fission_pcode::PcodeOpcode::Return =>
                    {
                        exit = Some(target)
                    }
                    _ => anyhow::bail!("baseline control flow outside straight return scope"),
                }
            }
            if exit.is_some() {
                break;
            }
        }
        ensure!(exit.is_some(), "baseline never returned");
        let mut output_registers = vec![0u64; 17];
        for (slot, name) in names.iter().enumerate() {
            output_registers[slot] = read_word(&mut state, &registers[*name])?;
        }
        let output_flags: Vec<_> = flags
            .iter()
            .map(|name| read_word(&mut state, &registers[*name]))
            .collect::<Result<_>>()?;
        println!(
            "{}",
            serde_json::json!({"registers":output_registers,"flags":output_flags,"exit":exit,"scope":"actual Fission SLA plus canonical P-code interpreter; not a statement about P-code expressive limits"})
        );
    }
    Ok(())
}
