//! Experimental FIR decompilation entry point, separate from product routing.
use fission_fsl::{
    x86::{X86Package, X86Program},
    MachineState, OutputLayer,
};
use std::{fs, path::Path, process::ExitCode};

fn number(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(if let Some(s) = s.strip_prefix("0x") {
        u64::from_str_radix(s, 16)?
    } else {
        s.parse()?
    })
}
fn bytes(s: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err("hex requires pairs of ASCII digits".into());
    }
    Ok((0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<_, _>>()?)
}
fn words(s: &str) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    s.split(',').map(number).collect()
}
fn layer(s: &str) -> Result<OutputLayer, Box<dyn std::error::Error>> {
    Ok(match s {
        "fir" => OutputLayer::Fir,
        "c" => OutputLayer::C,
        "rust" => OutputLayer::Rust,
        _ => return Err("expected fir|c|rust".into()),
    })
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    let usage = concat!(
        "usage: fsl-x86 compile RULES.fslx BODIES.fsl OUTPUT.fslxc | ",
        "decompile PACKAGE MODE BASE HEX fir|c|rust OUTPUT | ",
        "decompile-binary PACKAGE BINARY ADDRESS SIZE fir|c|rust OUTPUT | ",
        "execute PACKAGE MODE BASE HEX REGISTERS_CSV FLAGS_CSV MEMORY_BASE MEMORY_HEX BUDGET | ",
        "execute-binary PACKAGE BINARY ADDRESS SIZE REGISTERS_CSV FLAGS_CSV MEMORY_BASE MEMORY_HEX BUDGET"
    );
    match a.first().map(String::as_str) {
        Some("compile") if a.len() == 4 => {
            let p = X86Package::compile(&fs::read_to_string(&a[1])?, &fs::read_to_string(&a[2])?)?;
            let data = p.encode_binary()?;
            fs::write(&a[3], data)?;
            println!(
                "compiled x86 rules={} canonical-FIR-bodies={} into {}",
                p.rule_count(),
                p.bodies().instructions.len(),
                a[3]
            );
        }
        Some("decompile") if a.len() == 7 => {
            let p = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program = X86Program::lift(&p, a[2].parse()?, number(&a[3])?, &bytes(&a[4])?)?;
            fs::write(&a[6], program.emit(layer(&a[5])?)?)?;
            println!("emitted FIR projection into {}", a[6]);
        }
        Some("decompile-binary") if a.len() == 7 => {
            let p = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program =
                X86Program::from_binary(&p, Path::new(&a[2]), number(&a[3])?, a[4].parse()?)?;
            fs::write(&a[6], program.emit(layer(&a[5])?)?)?;
            println!("emitted loader-backed FIR projection into {}", a[6]);
        }
        Some("execute" | "execute-binary") if a.len() == 10 => {
            let p = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program = if a[0] == "execute-binary" {
                X86Program::from_binary(&p, Path::new(&a[2]), number(&a[3])?, a[4].parse()?)?
            } else {
                X86Program::lift(&p, a[2].parse()?, number(&a[3])?, &bytes(&a[4])?)?
            };
            let mut state = MachineState {
                registers: words(&a[5])?,
                flags: words(&a[6])?,
            };
            let (status, exit) =
                program.execute(&mut state, &bytes(&a[8])?, number(&a[7])?, a[9].parse()?)?;
            println!(
                "status={status:?} exit={exit:?} registers={:?} flags={:?}",
                state.registers, state.flags
            );
            if status != fission_fsl::ExecutionStatus::Success {
                return Err("execution refused; state preserved".into());
            }
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fsl-x86: {e}");
            ExitCode::FAILURE
        }
    }
}
