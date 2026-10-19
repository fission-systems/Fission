//! Experimental FIR decompilation entry point, separate from product routing.
use fission_fsl::{
    backend::{BackendRegistry, ProjectionInput, TargetProfile},
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
        "compile-profiles SOURCE.fslprofiles OUTPUT.fslpc | ",
        "inspect-profiles CATALOG.fslpc | select-profile CATALOG.fslpc BINARY | ",
        "decompile-profiled CATALOG.fslpc BINARY ADDRESS SIZE c|rust|go NEW_DIRECTORY | ",
        "decompile PACKAGE MODE BASE HEX fir|c|rust OUTPUT | ",
        "decompile-owned BINARY ADDRESS SIZE c|rust|go NEW_DIRECTORY | ",
        "decompile-owned-entry BINARY ADDRESS c|rust|go NEW_DIRECTORY | ",
        "decompile-binary PACKAGE BINARY ADDRESS SIZE fir|c|rust OUTPUT | ",
        "recover-binary PACKAGE BINARY ADDRESS SIZE OUTPUT | ",
        "plan-binary PACKAGE BINARY ADDRESS SIZE c|rust|go | ",
        "bundle-binary PACKAGE BINARY ADDRESS SIZE c|rust|go NEW_DIRECTORY | ",
        "execute-memory-binary PACKAGE BINARY ADDRESS SIZE REGISTERS_CSV FLAGS_CSV MEMORY_BASE MEMORY_HEX BUDGET | ",
        "execute PACKAGE MODE BASE HEX REGISTERS_CSV FLAGS_CSV MEMORY_BASE MEMORY_HEX BUDGET | ",
        "execute-binary PACKAGE BINARY ADDRESS SIZE REGISTERS_CSV FLAGS_CSV MEMORY_BASE MEMORY_HEX BUDGET"
    );
    match a.first().map(String::as_str) {
        Some("compile-profiles") if a.len() == 3 => {
            let profiles =
                fission_fsl::profiles::ProfilePackage::compile(&fs::read_to_string(&a[1])?)?;
            fs::write(&a[2], profiles.encode_binary()?)?;
            print!("{}", profiles.describe());
        }
        Some("inspect-profiles") if a.len() == 2 => {
            let profiles = fission_fsl::profiles::ProfilePackage::decode_binary(&fs::read(&a[1])?)?;
            print!("{}", profiles.describe());
        }
        Some("select-profile") if a.len() == 3 => {
            let profiles = fission_fsl::profiles::ProfilePackage::decode_binary(&fs::read(&a[1])?)?;
            let loaded = fission_loader::LoadedBinary::from_file_with_catalog(
                Path::new(&a[2]),
                &profiles.catalog,
            )?;
            println!("selected-profile={} decoder={} package-sha256={} abi=unselected legacy-load-spec={} target-facts={:?}",loaded.profile.id,loaded.profile.decoder,loaded.profile.package_sha256,loaded.binary.load_spec().is_some(),loaded.target_facts);
        }
        Some("decompile-profiled") if a.len() == 7 => {
            let profiles = fission_fsl::profiles::ProfilePackage::decode_binary(&fs::read(&a[1])?)?;
            let bundle = fission_fsl::pipeline::project_profiled_x86(
                &profiles,
                Path::new(&a[2]),
                number(&a[3])?,
                a[4].parse()?,
                &a[5],
            )?;
            bundle.write_new(Path::new(&a[6]))?;
            println!("emitted catalog-selected FIR bundle into {}", a[6]);
        }
        Some("decompile-owned-entry") if a.len() == 5 => {
            let bundle =
                fission_fsl::pipeline::project_x86_entry(Path::new(&a[1]), number(&a[2])?, &a[3])?;
            bundle.write_new(Path::new(&a[4]))?;
            println!("emitted conservative owned entry bundle into {}", a[4]);
        }
        Some("decompile-owned") if a.len() == 6 => {
            let bundle = fission_fsl::pipeline::project_x86_binary(
                Path::new(&a[1]),
                number(&a[2])?,
                a[3].parse()?,
                &a[4],
            )?;
            bundle.write_new(Path::new(&a[5]))?;
            println!("emitted owned FSL/FIR pipeline bundle into {}", a[5]);
        }
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
        Some("plan-binary" | "bundle-binary")
            if a.len() == if a[0] == "plan-binary" { 6 } else { 7 } =>
        {
            let registry = BackendRegistry::with_builtin_sources();
            let backend = registry
                .get(&a[5])
                .ok_or("unregistered source backend; available: c, rust, go")?;
            let package = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program =
                X86Program::from_binary(&package, Path::new(&a[2]), number(&a[3])?, a[4].parse()?)?;
            let plan = backend.plan(
                ProjectionInput::X86Window(&program),
                &TargetProfile::for_backend(backend),
            )?;
            if a[0] == "plan-binary" {
                print!("{}", plan.describe());
            } else {
                let bundle = backend.emit(&plan)?;
                // A new directory avoids overwriting existing user artifacts.
                let folder = Path::new(&a[6]);
                fs::create_dir(folder)?;
                for source in bundle.sources {
                    fs::write(folder.join(source.name), source.source)?;
                }
                fs::write(folder.join("projection.txt"), bundle.report)?;
                fs::write(
                    folder.join("build.txt"),
                    format!(
                        "tool={}\narguments={:?}\nexecution=not-performed\n",
                        bundle.build.tool, bundle.build.arguments
                    ),
                )?;
                println!("emitted checked source bundle into {}", folder.display());
            }
        }
        Some("recover-binary") if a.len() == 6 => {
            let p = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program =
                X86Program::from_binary(&p, Path::new(&a[2]), number(&a[3])?, a[4].parse()?)?;
            fs::write(&a[5], program.recover_input_evidence()?)?;
            println!("emitted bounded input evidence into {}", a[5]);
        }
        Some("execute" | "execute-binary" | "execute-memory-binary") if a.len() == 10 => {
            let p = X86Package::decode_binary(&fs::read(&a[1])?)?;
            let program = if a[0] != "execute" {
                X86Program::from_binary(&p, Path::new(&a[2]), number(&a[3])?, a[4].parse()?)?
            } else {
                X86Program::lift(&p, a[2].parse()?, number(&a[3])?, &bytes(&a[4])?)?
            };
            let mut state = MachineState {
                registers: words(&a[5])?,
                flags: words(&a[6])?,
            };
            let mut memory = bytes(&a[8])?;
            let (status, exit) = if a[0] == "execute-memory-binary" {
                program.execute_mut(&mut state, &mut memory, number(&a[7])?, a[9].parse()?)?
            } else {
                program.execute(&mut state, &memory, number(&a[7])?, a[9].parse()?)?
            };
            println!(
                "status={status:?} exit={exit:?} registers={:?} flags={:?}",
                state.registers, state.flags
            );
            if a[0] == "execute-memory-binary" {
                println!(
                    "memory={}",
                    memory
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                );
            }
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
