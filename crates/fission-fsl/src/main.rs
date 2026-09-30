use std::env;
use std::fs;
use std::process::ExitCode;

use fission_fsl::{compile_source, FirOp, FslcPackage, JitDecoder};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fslc: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    match command.as_str() {
        "check" => {
            let source_path = required_arg(&mut args, "source path")?;
            reject_extra_args(args)?;
            let source = fs::read_to_string(&source_path)?;
            let package = compile_source(&source)?;
            println!(
                "valid FSL: {} ({} instructions, {} byte order)",
                package.language,
                package.instructions.len(),
                match package.byte_order {
                    fission_fsl::ByteOrder::Little => "little",
                    fission_fsl::ByteOrder::Big => "big",
                }
            );
        }
        "compile" => {
            let source_path = required_arg(&mut args, "source path")?;
            let output_path = required_arg(&mut args, "output .fslc path")?;
            reject_extra_args(args)?;
            let source = fs::read_to_string(&source_path)?;
            let package = compile_source(&source)?;
            let bytes = package.encode_binary()?;
            fs::write(&output_path, &bytes)?;
            println!(
                "compiled {} instruction(s) for {} into {} ({} bytes)",
                package.instructions.len(),
                package.language,
                output_path,
                bytes.len()
            );
        }
        "inspect" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            reject_extra_args(args)?;
            let package = load_package(&package_path)?;
            print_package(&package);
        }
        "decode" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            let byte = required_arg(&mut args, "opcode byte")?;
            reject_extra_args(args)?;
            let package = load_package(&package_path)?;
            let opcode = parse_byte(&byte)?;
            let instruction = package
                .instruction_for_opcode(opcode)
                .ok_or_else(|| format!("no instruction for opcode 0x{opcode:02x}"))?;
            println!(
                "{}  opcode=0x{:02x}  mnemonic={}",
                instruction.name, opcode, instruction.mnemonic
            );
            for op in &instruction.ops {
                match op {
                    FirOp::VmStackPop { output } => {
                        let value = &instruction.values[output.0 as usize];
                        println!("  %{:<8} : {} = vm.stack.pop", value.name, value.ty);
                    }
                    FirOp::IntAddWrap {
                        output,
                        left,
                        right,
                    } => {
                        let output = &instruction.values[output.0 as usize];
                        let left = &instruction.values[left.0 as usize];
                        let right = &instruction.values[right.0 as usize];
                        println!(
                            "  %{:<8} : {} = int.add.wrap %{}, %{}",
                            output.name, output.ty, left.name, right.name
                        );
                    }
                    FirOp::VmStackPush { value } => {
                        let value = &instruction.values[value.0 as usize];
                        println!("  vm.stack.push %{}", value.name);
                    }
                }
            }
        }
        "jit-decode" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            let byte = required_arg(&mut args, "opcode-hex")?;
            reject_extra_args(args)?;
            let package = load_package(&package_path)?;
            let opcode = parse_byte(&byte)?;
            let jit = JitDecoder::compile(&package)?;
            let Some(lift) = jit.decode_and_lift(&[opcode])? else {
                return Err(format!("no instruction for opcode 0x{opcode:02x}").into());
            };
            let instruction = &package.instructions[lift.instruction_index];
            println!(
                "native FIR lift: {}  opcode=0x{:02x}  {} operation(s)",
                instruction.mnemonic,
                opcode,
                lift.ops.len()
            );
            for op in lift.ops {
                println!(
                    "  kind={} out={} in0={} in1={} type={}{}",
                    op.kind,
                    op.output,
                    op.input0,
                    op.input1,
                    if op.signed != 0 { 'i' } else { 'u' },
                    op.bits
                );
            }
        }
        _ => {
            print_usage();
            return Err(format!("unknown command {command:?}").into());
        }
    }
    Ok(())
}

fn load_package(path: &str) -> Result<FslcPackage, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(FslcPackage::decode_binary(&bytes)?)
}

fn print_package(package: &FslcPackage) {
    println!("FSL package v{}: {}", package.version, package.language);
    for instruction in &package.instructions {
        println!(
            "  {}  opcode=0x{:02x}  mnemonic={}",
            instruction.name, instruction.opcode, instruction.mnemonic
        );
        println!(
            "    FIR values: {}, operations: {}",
            instruction.values.len(),
            instruction.ops.len()
        );
    }
}

fn parse_byte(value: &str) -> Result<u8, Box<dyn std::error::Error>> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let byte = u8::from_str_radix(digits, 16)?;
    Ok(byte)
}

fn required_arg(
    args: &mut impl Iterator<Item = String>,
    description: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next()
        .ok_or_else(|| format!("missing {description}").into())
}

fn reject_extra_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument {extra:?}").into());
    }
    Ok(())
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  fslc check <source.fsl>");
    eprintln!("  fslc compile <source.fsl> <output.fslc>");
    eprintln!("  fslc inspect <package.fslc>");
    eprintln!("  fslc decode <package.fslc> <opcode-hex>");
    eprintln!("  fslc jit-decode <package.fslc> <opcode-hex>");
}
