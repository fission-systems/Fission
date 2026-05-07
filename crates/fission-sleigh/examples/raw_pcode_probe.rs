use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::compiler::{
    load_construct_templates_from_sla, resolve_ghidra_install_paths, CompiledSpaceRef,
};
use fission_sleigh::runtime::{DecodedInstruction, RuntimeSleighFrontend};
use serde::Serialize;

/// Maps Fission's internal dense `space_id` → Ghidra-native `CompiledSpaceRef`.
/// Built from the SLA template library's canonical space definitions so the
/// probe output matches Ghidra's own raw P-code dump exactly.
#[derive(Debug, Clone)]
struct SpaceMap {
    /// space_id (SLA native index) → CompiledSpaceRef {name, index}
    by_id: BTreeMap<u64, CompiledSpaceRef>,
}

impl SpaceMap {
    fn from_sla_spaces(spaces: &BTreeMap<u64, CompiledSpaceRef>) -> Self {
        Self {
            by_id: spaces.clone(),
        }
    }

    fn empty() -> Self {
        let mut by_id = BTreeMap::new();
        by_id.insert(
            0,
            CompiledSpaceRef {
                name: "const".to_string(),
                index: 0,
                word_size: 0,
                addr_size: 0,
            },
        );
        Self { by_id }
    }

    fn resolve_name(&self, space_id: u64) -> String {
        self.by_id
            .get(&space_id)
            .map(|space| space.name.clone())
            .unwrap_or_else(|| format!("space_{space_id}"))
    }

    /// For LOAD/STORE, the first input is a constant whose value is the space
    /// being accessed. Fission stores its internal dense index; Ghidra stores
    /// the native SLA index. This translates from the internal value to the
    /// native Ghidra `spaceId` integer that the oracle comparison expects.
    ///
    /// In practice, the SLA parser sets `CompiledSpaceRef.index` to the native
    /// SLA index, and the emitter stores that same value in `Varnode.space_id`.
    /// For `LOAD`/`STORE` constant inputs, the value IS the space index, so we
    /// must map it back to the SLA-native `spaceId` that Ghidra uses — which
    /// is actually the "unique space ID" as Ghidra's `AddrSpace.getSpaceID()`
    /// encodes it (typically `(type << 8) | index`).
    fn translate_load_store_space_constant(&self, constant_val: i64) -> i64 {
        let internal_id = constant_val as u64;
        if let Some(space) = self.by_id.get(&internal_id) {
            // The SLA native index IS the Ghidra spaceId for raw p-code.
            // Ghidra's getSpaceID() returns a packed value, but for raw p-code
            // comparison the oracle just uses the native index from the SLA.
            // The space.index from the SLA decode_spaces is exactly this.
            space.index as i64
        } else {
            constant_val
        }
    }
}

#[derive(Debug, Serialize)]
struct ProbeReport {
    binary: String,
    language_id: Option<String>,
    compiler_spec_id: Option<String>,
    entry_id: String,
    execution_mode: String,
    start_address: u64,
    requested_count: usize,
    space_map: BTreeMap<String, u64>,
    timing: ProbeTiming,
    instructions: Vec<InstructionReport>,
}

#[derive(Debug, Serialize)]
struct ProbeTiming {
    rust_probe_sec: f64,
    binary_load_sec: f64,
    frontend_load_sec: f64,
    decode_lift_sec: f64,
}

#[derive(Debug, Serialize)]
struct InstructionReport {
    address: u64,
    status: String,
    error: Option<String>,
    decoded: Option<DecodedInstruction>,
    length: Option<u64>,
    template_source: Option<String>,
    pcode: Vec<SerializablePcodeOp>,
}

#[derive(Debug, Serialize)]
struct SerializablePcodeOp {
    seq_num: u32,
    opcode: String,
    address: u64,
    output: Option<SerializableVarnode>,
    inputs: Vec<SerializableVarnode>,
}

#[derive(Debug, Serialize)]
struct SerializableVarnode {
    space: String,
    space_id: u64,
    offset: u64,
    size: u32,
    is_constant: bool,
    constant_val: i64,
}

impl SerializableVarnode {
    fn from_varnode(vn: &Varnode, space_map: &SpaceMap) -> Self {
        Self {
            space: if vn.is_constant {
                "const".to_string()
            } else {
                space_map.resolve_name(vn.space_id)
            },
            space_id: vn.space_id,
            offset: vn.offset,
            size: vn.size,
            is_constant: vn.is_constant,
            constant_val: vn.constant_val,
        }
    }

    fn from_varnode_load_store_space(vn: &Varnode, space_map: &SpaceMap) -> Self {
        // For LOAD/STORE input[0], translate the constant space reference
        if vn.is_constant {
            let translated = space_map.translate_load_store_space_constant(vn.constant_val);
            Self {
                space: "const".to_string(),
                space_id: 0,
                offset: translated as u64,
                size: vn.size,
                is_constant: true,
                constant_val: translated,
            }
        } else {
            Self::from_varnode(vn, space_map)
        }
    }
}

impl SerializablePcodeOp {
    fn from_op(op: &PcodeOp, space_map: &SpaceMap) -> Self {
        let is_load_store = matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store);
        let inputs: Vec<SerializableVarnode> = op
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, vn)| {
                if is_load_store && idx == 0 {
                    SerializableVarnode::from_varnode_load_store_space(vn, space_map)
                } else {
                    SerializableVarnode::from_varnode(vn, space_map)
                }
            })
            .collect();

        Self {
            seq_num: op.seq_num,
            opcode: format!("{:?}", op.opcode),
            address: op.address,
            output: op
                .output
                .as_ref()
                .map(|vn| SerializableVarnode::from_varnode(vn, space_map)),
            inputs,
        }
    }
}

#[derive(Debug)]
struct Args {
    binary: PathBuf,
    address: u64,
    count: usize,
    window_bytes: usize,
}

fn load_space_map(frontend: &RuntimeSleighFrontend) -> SpaceMap {
    // Try to load the SLA template library to get authoritative space definitions
    let entry = frontend.entry();
    let sla_path = find_packaged_sla(&entry.entry_id);
    match sla_path {
        Some(path) => match load_construct_templates_from_sla(&path) {
            Ok(library) => SpaceMap::from_sla_spaces(&library.spaces),
            Err(_) => SpaceMap::empty(),
        },
        None => SpaceMap::empty(),
    }
}

fn find_packaged_sla(entry_id: &str) -> Option<PathBuf> {
    let paths = resolve_ghidra_install_paths()?;
    let wanted_name = format!("{entry_id}.sla");
    let mut matches = Vec::new();
    find_named_file(&paths.processors_root, &wanted_name, &mut matches).ok()?;
    matches.sort();
    matches.into_iter().next()
}

fn find_named_file(root: &std::path::Path, name: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        std::fs::read_dir(root).with_context(|| format!("read directory {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("read entry under {}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type for {}", path.display()))?;
        if file_type.is_dir() {
            find_named_file(&path, name, out)?;
        } else if path.file_name().and_then(|value| value.to_str()) == Some(name) {
            out.push(path);
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let probe_started = Instant::now();
    let args = parse_args()?;

    let binary_load_started = Instant::now();
    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to load {}", args.binary.display()))?;
    let binary_load_sec = binary_load_started.elapsed().as_secs_f64();

    let load_spec = binary
        .load_spec()
        .ok_or_else(|| anyhow!("loader did not select a load spec"))?;

    let frontend_load_started = Instant::now();
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec)?;
    let space_map = load_space_map(&frontend);
    let frontend_load_sec = frontend_load_started.elapsed().as_secs_f64();

    let mut instructions = Vec::new();
    let mut current = args.address;
    let mut decode_lift_sec = 0.0f64;
    for _ in 0..args.count {
        let Some(bytes) = binary
            .view_executable_bytes(current, args.window_bytes)
            .or_else(|| binary.view_bytes(current, args.window_bytes))
        else {
            instructions.push(InstructionReport {
                address: current,
                status: "error".to_string(),
                error: Some(format!("unable to read bytes at 0x{current:x}")),
                decoded: None,
                length: None,
                template_source: None,
                pcode: Vec::new(),
            });
            break;
        };

        let decode_lift_started = Instant::now();
        let decoded = frontend.decode_window(bytes, current, 1);
        let lifted = frontend.decode_and_lift_with_details(bytes, current);
        decode_lift_sec += decode_lift_started.elapsed().as_secs_f64();
        match lifted {
            Ok((ops, len, details)) => {
                instructions.push(InstructionReport {
                    address: current,
                    status: "ok".to_string(),
                    error: None,
                    decoded: decoded.ok().and_then(|mut window| window.pop()),
                    length: Some(len),
                    template_source: details
                        .template_source
                        .map(|source| format!("{:?}", source)),
                    pcode: ops
                        .iter()
                        .map(|op| SerializablePcodeOp::from_op(op, &space_map))
                        .collect(),
                });
                if len == 0 {
                    break;
                }
                current = current.saturating_add(len);
            }
            Err(err) => {
                instructions.push(InstructionReport {
                    address: current,
                    status: "error".to_string(),
                    error: Some(err.to_string()),
                    decoded: decoded.ok().and_then(|mut window| window.pop()),
                    length: None,
                    template_source: None,
                    pcode: Vec::new(),
                });
                break;
            }
        }
    }

    // Export space_map for diagnostic visibility
    let space_map_json: BTreeMap<String, u64> = space_map
        .by_id
        .iter()
        .map(|(id, space)| (space.name.clone(), *id))
        .collect();

    let report = ProbeReport {
        binary: args.binary.display().to_string(),
        language_id: Some(load_spec.pair.language_id.as_str().to_string()),
        compiler_spec_id: Some(load_spec.pair.compiler_spec_id.as_str().to_string()),
        entry_id: frontend.entry().entry_id.clone(),
        execution_mode: "compiled_table_mixed".to_string(),
        start_address: args.address,
        requested_count: args.count,
        space_map: space_map_json,
        timing: ProbeTiming {
            rust_probe_sec: probe_started.elapsed().as_secs_f64(),
            binary_load_sec,
            frontend_load_sec,
            decode_lift_sec,
        },
        instructions,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut binary = None;
    let mut address = None;
    let mut count = 8usize;
    let mut window_bytes = 32usize;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--binary" => binary = iter.next().map(PathBuf::from),
            "--addr" | "--address" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                address = Some(parse_u64(&raw)?);
            }
            "--count" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                count = raw
                    .parse()
                    .with_context(|| format!("invalid count: {raw}"))?;
            }
            "--window-bytes" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                window_bytes = raw
                    .parse()
                    .with_context(|| format!("invalid window byte count: {raw}"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(Args {
        binary: binary.ok_or_else(|| anyhow!("missing --binary"))?,
        address: address.ok_or_else(|| anyhow!("missing --addr"))?,
        count,
        window_bytes,
    })
}

fn parse_u64(raw: &str) -> Result<u64> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).with_context(|| format!("invalid hex address: {raw}"))
    } else {
        raw.parse()
            .with_context(|| format!("invalid decimal address: {raw}"))
    }
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p fission-sleigh --example raw_pcode_probe -- \\
  --binary <path> --addr <hex-or-decimal> [--count N] [--window-bytes N]"
    );
}
