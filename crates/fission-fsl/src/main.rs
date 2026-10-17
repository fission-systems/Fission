use std::env;
use std::fs;
use std::process::ExitCode;

use fission_fsl::{
    compile_source, emit_aot_object, emit_instruction, execute_decoded, execute_instruction,
    execute_wave, FslcPackage, JitDecoder, MachineState, OutputLayer, WaveState,
};

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
        "run-fabric" => {
            let package = load_package(&required_arg(&mut args, "compiled .fslc")?)?;
            let name = required_arg(&mut args, "instruction name")?;
            let input = required_arg(&mut args, "input slot rows CSV")?;
            let output = required_arg(&mut args, "output slot rows CSV")?;
            reject_extra_args(args)?;
            let body = package
                .instructions
                .iter()
                .find(|body| body.name == name)
                .ok_or("unknown FIR body")?;
            let plan = fission_fsl::fabric::FabricPlan::new(body)?;
            let bytes = fs::read(input)?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("fabric reference input exceeds 8 MiB".into());
            }
            let text = std::str::from_utf8(&bytes)?;
            let mut rows = Vec::new();
            for line in text.lines() {
                if rows.len() >= 65536 {
                    return Err("fabric reference exceeds 65536 rows".into());
                }
                let inputs = if line == "-" {
                    vec![]
                } else {
                    line.split(',')
                        .map(str::parse::<u64>)
                        .collect::<Result<Vec<_>, _>>()?
                };
                rows.push(
                    plan.evaluate(&inputs)?
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            if rows.is_empty() {
                return Err("fabric reference requires a nonempty row set".into());
            }
            fs::write(output, rows.join("\n") + "\n")?;
            println!("{{\"rows\":{},\"execution\":\"classical_reference\",\"target_hardware_execution\":false}}", rows.len());
        }
        "fabric-axes" => {
            reject_extra_args(args)?;
            let axes = fission_fsl::fabric::AXES
                .iter()
                .map(|axis| {
                    format!(
                        "{{\"id\":\"{}\",\"initial_slice\":\"{}\",\"unsupported\":\"{}\"}}",
                        axis.id, axis.initial_slice, axis.unsupported
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            println!("{{\"axes\":[{axes}],\"target_hardware_execution\":false}}");
        }
        "emit-fabric" | "emit-dataflow" => {
            let package = load_package(&required_arg(&mut args, "compiled .fslc")?)?;
            let name = required_arg(&mut args, "instruction name")?;
            let symbol = required_arg(&mut args, "fsl_ module or graph symbol")?;
            let output = required_arg(&mut args, "output artifact")?;
            reject_extra_args(args)?;
            let body = package
                .instructions
                .iter()
                .find(|body| body.name == name)
                .ok_or("unknown FIR body")?;
            let plan = fission_fsl::fabric::FabricPlan::new(body)?;
            let text = if command == "emit-fabric" {
                plan.emit_verilog(&symbol)?
            } else {
                plan.emit_dot(&symbol)?
            };
            fs::write(output, text)?;
            println!("{{\"input_slots\":{},\"output_slots\":{},\"combinational\":true,\"target_hardware_execution\":false}}", plan.input_slots(), plan.output_slots());
        }
        "emit-ibm" => {
            use fission_fsl::quantum::ibm::{emit, Entangler, Target};
            let package = load_package(&required_arg(&mut args, "compiled .fslc")?)?;
            let name = required_arg(&mut args, "instruction name")?;
            let layout = required_arg(&mut args, "physical qubits CSV")?;
            let physical_qubits = layout
                .split(',')
                .map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>()?;
            let entangler = match required_arg(&mut args, "cx or cz")?.as_str() {
                "cx" => Entangler::Cx,
                "cz" => Entangler::Cz,
                _ => return Err("IBM entangler must be cx or cz".into()),
            };
            let edges = required_arg(&mut args, "physical couplings a:b CSV, or -")?;
            let couplings = if edges == "-" {
                vec![]
            } else {
                edges
                    .split(',')
                    .map(|edge| -> Result<(u16, u16), Box<dyn std::error::Error>> {
                        let (a, b) = edge.split_once(':').ok_or("coupling must be a:b")?;
                        Ok((a.parse()?, b.parse()?))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            let initial_basis = required_arg(&mut args, "explicit initial basis index")?.parse()?;
            let observation = required_arg(&mut args, "logical X/Y/Z observation axes")?;
            let output = required_arg(&mut args, "output physical .qasm")?;
            reject_extra_args(args)?;
            let body = package
                .instructions
                .iter()
                .find(|i| i.name == name)
                .ok_or("unknown quantum instruction")?;
            let target = Target {
                physical_qubits,
                entangler,
                couplings,
            };
            let circuit = emit(body, &target, initial_basis, &observation)?;
            fs::write(output, &circuit.qasm)?;
            let ids = circuit
                .trace_values
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(",");
            println!("{{\"trace_values\":[{ids}],\"hardware_execution\":false,\"target_verified\":false}}");
        }
        "import-qasm" => {
            let input = required_arg(&mut args, "restricted OpenQASM 3.0 source")?;
            let output = required_arg(&mut args, "output .fslc")?;
            reject_extra_args(args)?;
            let imported = fission_fsl::quantum::import_openqasm(&fs::read_to_string(input)?)?;
            fs::write(&output, imported.package.encode_binary()?)?;
            println!("imported logical circuit qubits={} into {output}; initial state remains an external precondition", imported.qubits);
        }
        "emit-qasm" | "simulate-quantum" | "simulate-quantum-bounded" => {
            let package = load_package(&required_arg(&mut args, "compiled .fslc")?)?;
            let name = required_arg(&mut args, "instruction name")?;
            let qubits = required_arg(&mut args, "logical qubit count")?.parse()?;
            let argument = required_arg(&mut args, "output .qasm or basis:N / vector:re:im,...")?;
            let limits = if command == "simulate-quantum-bounded" {
                Some(fission_fsl::quantum::QuantumLimits {
                    steps: required_arg(&mut args, "global step budget")?.parse()?,
                    splits: required_arg(&mut args, "per-path split budget")?.parse()?,
                    branches: required_arg(&mut args, "retained branch capacity")?.parse()?,
                })
            } else {
                None
            };
            reject_extra_args(args)?;
            let body = package
                .instructions
                .iter()
                .find(|i| i.name == name)
                .ok_or("unknown quantum instruction")?;
            if command == "emit-qasm" {
                fs::write(argument, fission_fsl::quantum::emit_openqasm(body, qubits)?)?;
            } else {
                use fission_fsl::quantum::{Complex, PureState};
                let initial = if let Some(index) = argument.strip_prefix("basis:") {
                    PureState::basis(qubits, index.parse()?)?
                } else if let Some(vector) = argument.strip_prefix("vector:") {
                    let amplitudes = vector
                        .split(',')
                        .map(|pair| -> Result<Complex, Box<dyn std::error::Error>> {
                            let (re, im) =
                                pair.split_once(':').ok_or("expected re:im amplitude")?;
                            Ok(Complex {
                                re: re.parse()?,
                                im: im.parse()?,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    PureState::new(qubits, amplitudes)?
                } else {
                    return Err("initial state must be explicit basis:N or vector:re:im,...".into());
                };
                if let Some(limits) = limits {
                    let run = fission_fsl::quantum::simulate_bounded(body, &initial, limits)?;
                    print!("{}", run.diagnostic_json());
                    if !run.is_complete() {
                        return Err("incomplete quantum execution: resource limit reached; inspect retained frontier".into());
                    }
                } else {
                    let result = fission_fsl::quantum::simulate(body, &initial)?;
                    print!("{}", result.diagnostic_json());
                }
            }
        }
        "compose-sequence" => {
            let package = load_package(&required_arg(&mut args, "compiled .fslc path")?)?;
            let profile = required_arg(&mut args, "exact profile")?;
            let base = required_arg(&mut args, "base address")?;
            let base = if let Some(hex) = base.strip_prefix("0x") {
                u64::from_str_radix(hex, 16)?
            } else {
                base.parse()?
            };
            let register_count = required_arg(&mut args, "register bank size")?.parse()?;
            let flag_count = required_arg(&mut args, "flag bank size")?.parse()?;
            let input = parse_bytes(&required_arg(&mut args, "exact instruction window in hex")?)?;
            let output = required_arg(&mut args, "output .fslseq path")?;
            reject_extra_args(args)?;
            let sequence = fission_fsl::sequence::FirSequence::compose(
                &package,
                &profile,
                base,
                &input,
                fission_fsl::sequence::SequenceStateContract {
                    register_count,
                    flag_count,
                },
            )?;
            fs::write(&output, sequence.encode_binary()?)?;
            println!(
                "composed {} instruction instances into {output}",
                sequence.instances().len()
            );
        }
        "inspect-sequence" | "execute-sequence" | "emit-sequence" => {
            let path = required_arg(&mut args, "compiled .fslseq path")?;
            let sequence = fission_fsl::sequence::FirSequence::decode_binary(&fs::read(path)?)?;
            match command.as_str() {
                "inspect-sequence" => {
                    reject_extra_args(args)?;
                    print!("{}", sequence.emit(OutputLayer::Fir, "ignored")?);
                }
                "execute-sequence" => {
                    let registers = parse_words(&required_arg(&mut args, "register values csv")?)?;
                    let flags = parse_words(&required_arg(&mut args, "flag values csv")?)?;
                    reject_extra_args(args)?;
                    let mut state = MachineState { registers, flags };
                    let status = sequence.execute(&mut state)?;
                    println!(
                        "status={status:?} registers={:?} flags={:?}",
                        state.registers, state.flags
                    );
                }
                _ => {
                    let layer = match required_arg(&mut args, "fir|c|rust")?.as_str() {
                        "fir" => OutputLayer::Fir,
                        "c" => OutputLayer::C,
                        "rust" => OutputLayer::Rust,
                        _ => return Err("sequence output supports fir|c|rust only".into()),
                    };
                    let output = required_arg(&mut args, "source output path")?;
                    reject_extra_args(args)?;
                    fs::write(output, sequence.emit(layer, "fsl_sequence")?)?;
                }
            }
        }
        "library-inspect" => {
            let path = required_arg(&mut args, "library .fsldb path")?;
            reject_extra_args(args)?;
            let catalog = fission_fsl::library::LibraryCatalog::decode_binary(&fs::read(path)?)?;
            println!(
                "prototype-candidates={} type-resolution=unresolved source={} sha256={} commit={}",
                catalog.candidates.len(),
                catalog.source.path,
                catalog.source.sha256,
                catalog.source.commit
            );
        }
        "library-query" => {
            let path = required_arg(&mut args, "library .fsldb path")?;
            let symbol = required_arg(&mut args, "exact symbol")?;
            reject_extra_args(args)?;
            let catalog = fission_fsl::library::LibraryCatalog::decode_binary(&fs::read(path)?)?;
            let candidate = catalog
                .lookup(&symbol)
                .ok_or("no prototype candidate for symbol")?;
            println!("type-resolution=unresolved candidate={candidate:#?}");
        }
        "check-layout" => {
            let path = required_arg(&mut args, "layout FSL source path")?;
            reject_extra_args(args)?;
            let layout = fission_fsl::registers::compile_layout_source(&fs::read_to_string(path)?)?;
            println!(
                "valid layout: {} ({} spaces, {} register views)",
                layout.name,
                layout.spaces.len(),
                layout.registers.len()
            );
        }
        "link-abi" => {
            let abi_path = required_arg(&mut args, "ABI source path")?;
            let layout_path = required_arg(&mut args, "layout source path")?;
            reject_extra_args(args)?;
            let abi = fission_fsl::abi::compile_abi_source(&fs::read_to_string(abi_path)?)?;
            let layout =
                fission_fsl::registers::compile_layout_source(&fs::read_to_string(layout_path)?)?;
            let linked = fission_fsl::registers::link_abi(&abi, &layout)?;
            let stack = layout.view(linked.stack_register)?;
            println!(
                "linked ABI {} layout={} stack={}@{}:{}+{} conventions={}",
                abi.name,
                layout.name,
                stack.name,
                stack.space,
                stack.offset,
                stack.size_bytes,
                linked.conventions.len()
            );
            for convention in &linked.conventions {
                println!(
                    "convention {} inputs={:?} outputs={:?} preserved={:?} clobbered={:?}",
                    convention.name,
                    convention.inputs,
                    convention.outputs,
                    convention.preserved_registers,
                    convention.clobbered_registers
                );
            }
        }
        "execute-layout" => {
            use fission_fsl::registers::{
                compile_layout_source, execute_bound, link_abi, RegisterBinding, RegisterFile,
            };
            let layout_path = required_arg(&mut args, "layout source path")?;
            let abi_path = required_arg(&mut args, "ABI source path")?;
            let package_path = required_arg(&mut args, "package path")?;
            let profile = required_arg(&mut args, "profile")?;
            let hex = required_arg(&mut args, "instruction hex")?;
            let names = required_arg(&mut args, "register names CSV")?;
            let values = required_arg(&mut args, "register values CSV")?;
            reject_extra_args(args)?;
            let layout = compile_layout_source(&fs::read_to_string(layout_path)?)?;
            let abi = fission_fsl::abi::compile_abi_source(&fs::read_to_string(abi_path)?)?;
            link_abi(&abi, &layout)?;
            let binding = RegisterBinding {
                registers: names.split(',').map(str::to_owned).collect(),
                flags: Vec::new(),
            };
            let values = parse_words(&values)?;
            if values.len() != binding.registers.len() {
                return Err("register names/value counts differ".into());
            }
            let mut file = RegisterFile::new(layout)?;
            for (name, &value) in binding.registers.iter().zip(&values) {
                file.write_u64(name, value)?;
            }
            let package = load_package(&package_path)?;
            let decoded = package
                .decode_bytes(&profile, &parse_bytes(&hex)?)?
                .ok_or("no matching instruction")?;
            let status = execute_bound(&package, &decoded, &binding, &mut file)?;
            let registers = binding
                .registers
                .iter()
                .map(|name| file.read_u64(name))
                .collect::<Result<Vec<_>, _>>()?;
            println!("status={status:?} registers={registers:?}");
        }
        "check-abi" => {
            let source_path = required_arg(&mut args, "ABI FSL source path")?;
            reject_extra_args(args)?;
            let profile = fission_fsl::abi::compile_abi_source(&fs::read_to_string(source_path)?)?;
            println!("{profile:#?}");
        }
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
            print!(
                "{}",
                emit_instruction(instruction, OutputLayer::Fir, "fsl_execute")?
            );
        }
        "execute-wave" => {
            let path = required_arg(&mut args, "compiled .fslc path")?;
            let language = required_arg(&mut args, "exact input profile")?;
            let bytes = parse_bytes(&required_arg(&mut args, "instruction hex")?)?;
            let lanes = required_arg(&mut args, "lane count")?.parse::<u16>()?;
            let exec = parse_words(&required_arg(&mut args, "EXEC mask")?)?;
            if exec.len() != 1 {
                return Err("one EXEC mask required".into());
            }
            let registers = parse_words(&required_arg(&mut args, "scalar registers CSV")?)?;
            let flags = parse_words(&required_arg(&mut args, "flags CSV")?)?;
            let lane_registers =
                parse_words(&required_arg(&mut args, "register-major lane slots CSV")?)?;
            reject_extra_args(args)?;
            let package = load_package(&path)?;
            let decoded = package
                .decode_bytes(&language, &bytes)?
                .ok_or("unsupported encoding")?;
            if decoded.raw.len() != bytes.len() {
                return Err("one instruction required".into());
            }
            let mut state = WaveState {
                scalar: MachineState { registers, flags },
                lanes,
                exec: exec[0],
                lane_registers,
            };
            let status = execute_wave(&package, &decoded, &mut state)?;
            println!("status={status:?} lanes={} exec=0x{:x} registers={:?} flags={:?} lane_registers={:?}", state.lanes, state.exec, state.scalar.registers, state.scalar.flags, state.lane_registers);
        }
        "emit-backend" => {
            use fission_fsl::backend::{BackendRegistry, ProjectionInput, TargetProfile};
            let package = load_package(&required_arg(&mut args, "compiled .fslc path")?)?;
            let name = required_arg(&mut args, "instruction name")?;
            let backend_id = required_arg(&mut args, "registered backend id")?;
            let output = required_arg(&mut args, "output path")?;
            reject_extra_args(args)?;
            let registry = BackendRegistry::with_builtin_sources();
            let backend = registry
                .get(&backend_id)
                .ok_or("unregistered source backend")?;
            let body = package
                .instructions
                .iter()
                .find(|b| b.name == name)
                .ok_or("instruction not found")?;
            let plan = backend.plan(
                ProjectionInput::Instruction {
                    body,
                    symbol: "fsl_execute",
                },
                &TargetProfile::for_backend(backend),
            )?;
            let bundle = backend.emit(&plan)?;
            fs::write(output, &bundle.sources[0].source)?;
        }
        "execute-state" | "emit-bytes" => {
            let path = required_arg(&mut args, "compiled .fslc path")?;
            let language = required_arg(&mut args, "exact input profile")?;
            let bytes = parse_bytes(&required_arg(&mut args, "instruction bytes in hex")?)?;
            let package = load_package(&path)?;
            let decoded = package
                .decode_bytes(&language, &bytes)?
                .ok_or("unsupported encoding")?;
            if bytes.len() != decoded.raw.len() {
                return Err("one instruction required; trailing bytes rejected".into());
            }
            if command == "execute-state" {
                let registers = parse_words(&required_arg(
                    &mut args,
                    "comma-separated register bit vectors",
                )?)?;
                let flags = parse_words(&required_arg(&mut args, "comma-separated flag values")?)?;
                reject_extra_args(args)?;
                let mut state = MachineState { registers, flags };
                let status = execute_decoded(&package, &decoded, &mut state)?;
                println!(
                    "status={status:?} registers={:?} flags={:?}",
                    state.registers, state.flags
                );
            } else {
                let layer = match required_arg(&mut args, "output layer")?.as_str() {
                    "fir" => OutputLayer::Fir,
                    "c" => OutputLayer::C,
                    "rust" => OutputLayer::Rust,
                    "cuda" => OutputLayer::CudaCpp,
                    "ptx" => OutputLayer::Ptx,
                    _ => return Err("expected fir, c, rust, cuda or ptx".into()),
                };
                let output = required_arg(&mut args, "output path")?;
                reject_extra_args(args)?;
                fs::write(
                    output,
                    emit_instruction(
                        &package.instructions[decoded.instruction_index],
                        layer,
                        "fsl_execute",
                    )?,
                )?;
            }
        }
        "decode-bytes" => {
            let path = required_arg(&mut args, "compiled .fslc path")?;
            let language = required_arg(&mut args, "exact input profile")?;
            let bytes = parse_bytes(&required_arg(&mut args, "instruction bytes in hex")?)?;
            reject_extra_args(args)?;
            let package = load_package(&path)?;
            let decoded = package
                .decode_bytes(&language, &bytes)?
                .ok_or("unknown encoding or unsupported selector; stop decoding")?;
            println!(
                "profile={} consumed={} raw={:02x?}",
                decoded.language,
                decoded.raw.len(),
                decoded.raw
            );
            for (field, value) in &decoded.fields {
                println!("  {field}={value} (0x{value:x})");
            }
            print!(
                "{}",
                emit_instruction(
                    &package.instructions[decoded.instruction_index],
                    OutputLayer::Fir,
                    "fsl_execute"
                )?
            );
        }
        "reencode" => {
            let path = required_arg(&mut args, "compiled .fslc path")?;
            let language = required_arg(&mut args, "exact input profile")?;
            let bytes = parse_bytes(&required_arg(&mut args, "instruction bytes in hex")?)?;
            let output = required_arg(&mut args, "output binary path")?;
            let edits = args
                .map(|edit| {
                    let (name, value) = edit
                        .split_once('=')
                        .ok_or("field edit requires name=value")?;
                    let value = if let Some(hex) = value.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                    } else {
                        value.parse::<u64>()
                    }?;
                    Ok::<_, Box<dyn std::error::Error>>((name.to_owned(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let package = load_package(&path)?;
            let decoded = package
                .decode_bytes(&language, &bytes)?
                .ok_or("no supported encoding")?;
            let edits = edits
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect::<Vec<_>>();
            let encoded = package.reencode(&decoded, &edits)?;
            fs::write(output, &encoded)?;
            println!("reencoded raw={encoded:02x?}");
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
        "emit" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            let byte = required_arg(&mut args, "opcode byte")?;
            let layer =
                match required_arg(&mut args, "output layer (fir, c, rust, cuda, ptx)")?.as_str() {
                    "fir" => OutputLayer::Fir,
                    "c" => OutputLayer::C,
                    "rust" => OutputLayer::Rust,
                    "cuda" => OutputLayer::CudaCpp,
                    "ptx" => OutputLayer::Ptx,
                    layer => return Err(format!("unsupported output layer {layer:?}").into()),
                };
            let output_path = required_arg(&mut args, "output path")?;
            reject_extra_args(args)?;
            let package = load_package(&package_path)?;
            let opcode = parse_byte(&byte)?;
            let instruction = package
                .instruction_for_opcode(opcode)
                .ok_or_else(|| format!("no instruction for opcode 0x{opcode:02x}"))?;
            fs::write(
                output_path,
                emit_instruction(instruction, layer, "fsl_execute")?,
            )?;
        }
        "execute" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            let opcode = parse_byte(&required_arg(&mut args, "opcode byte")?)?;
            let capacity = required_arg(&mut args, "stack capacity")?.parse::<usize>()?;
            let mut stack = args
                .map(|value| {
                    if let Some(value) = value.strip_prefix("0x") {
                        u64::from_str_radix(value, 16)
                    } else {
                        value.parse::<u64>()
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let package = load_package(&package_path)?;
            let instruction = package
                .instruction_for_opcode(opcode)
                .ok_or_else(|| format!("no instruction for opcode 0x{opcode:02x}"))?;
            let status = execute_instruction(instruction, &mut stack, capacity)?;
            println!("status={status:?} stack={stack:?}");
        }
        "aot-object" => {
            let package_path = required_arg(&mut args, "compiled .fslc path")?;
            let output_path = required_arg(&mut args, "output object path")?;
            reject_extra_args(args)?;
            let package = load_package(&package_path)?;
            let object = emit_aot_object(&package)?;
            fs::write(&output_path, &object)?;
            println!(
                "emitted host-native object for {} to {} ({} bytes)",
                package.language,
                output_path,
                object.len()
            );
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
            "  {}  encoding={:?}  mnemonic={}",
            instruction.name, instruction.encoding, instruction.mnemonic
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

fn parse_bytes(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() || !value.len().is_multiple_of(2) || !value.is_ascii() {
        return Err("bytes must be a nonempty even-length ASCII hex string".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&value[index..index + 2], 16)?))
        .collect()
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
    eprintln!("  compose-sequence <package> <profile> <base-address> <register-count> <flag-count> <hex> <output.fslseq>\n  inspect-sequence <sequence.fslseq>\n  execute-sequence <sequence.fslseq> <registers-csv> <flags-csv>\n  emit-sequence <sequence.fslseq> <fir|c|rust> <output>");
    eprintln!("  execute-wave <package> <profile> <hex> <lanes> <exec> <scalar-csv> <flags-csv> <lane-slots-csv>");
    eprintln!("  execute-state <package> <profile> <hex> <registers-csv> <flags-csv>\n  emit-bytes <package> <profile> <hex> <fir|c|rust|cuda|ptx> <output>");
    eprintln!("usage:");
    eprintln!("  fslc fabric-axes");
    eprintln!("  fslc run-fabric PACKAGE NAME INPUT.csv OUTPUT.csv");
    eprintln!("  fslc emit-fabric|emit-dataflow PACKAGE NAME fsl_SYMBOL OUTPUT");
    eprintln!("  fslc import-qasm INPUT.qasm OUTPUT.fslc");
    eprintln!("  fslc emit-qasm PACKAGE NAME QUBITS OUTPUT.qasm");
    eprintln!("  fslc emit-ibm PACKAGE NAME PHYSICAL_CSV cx|cz COUPLINGS_CSV|- INITIAL_BASIS AXES OUTPUT.qasm");
    eprintln!("  fslc simulate-quantum PACKAGE NAME QUBITS basis:N|vector:re:im,...");
    eprintln!("  fslc simulate-quantum-bounded PACKAGE NAME QUBITS INITIAL STEPS SPLITS BRANCHES");
    eprintln!("  fslc emit-backend PACKAGE INSTRUCTION c|rust|go OUTPUT");
    eprintln!("  fslc library-inspect <library.fsldb>");
    eprintln!("  fslc library-query <library.fsldb> <exact-symbol>");
    eprintln!("  fslc check <source.fsl>");
    eprintln!("  fslc check-abi <source.fslabi>");
    eprintln!("  check-layout <source.fslregs>\n  link-abi <source.fslabi> <source.fslregs>\n  execute-layout <layout> <abi> <package> <profile> <hex> <register-names-csv> <register-values-csv>");
    eprintln!("  fslc compile <source.fsl> <output.fslc>");
    eprintln!("  fslc inspect <package.fslc>");
    eprintln!("  fslc decode <package.fslc> <opcode-hex>");
    eprintln!("  fslc decode-bytes <package.fslc> <profile> <bytes-hex>");
    eprintln!("  fslc reencode <package.fslc> <profile> <bytes-hex> <output.bin> [field=value...]");
    eprintln!("  fslc jit-decode <package.fslc> <opcode-hex>");
    eprintln!("  fslc aot-object <package.fslc> <output.o>");
    eprintln!("  fslc emit <package.fslc> <opcode-hex> <fir|c|rust|cuda|ptx> <output>");
    eprintln!("  fslc execute <package.fslc> <opcode-hex> <capacity> [stack bits...]");
}

fn parse_words(text: &str) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|value| {
            Ok(if let Some(hex) = value.strip_prefix("0x") {
                u64::from_str_radix(hex, 16)?
            } else {
                value.parse()?
            })
        })
        .collect()
}
