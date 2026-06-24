use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::compiler::{
    load_construct_templates_from_sla, packaged_sla_for_entry_spec, CompiledSpaceRef,
};
use fission_sleigh::runtime::{DecodedInstruction, PackedContextOverride, RuntimeSleighFrontend};
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
    decode_start_address: u64,
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

fn merge_context_overrides(
    base: PackedContextOverride,
    pending: PackedContextOverride,
) -> PackedContextOverride {
    let pending_mask = pending.mask_bits();
    PackedContextOverride::new(
        (base.context_bits() & !pending_mask) | (pending.context_bits() & pending_mask),
        base.mask_bits() | pending_mask,
    )
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
    let sla_path = find_packaged_sla(&entry.entry_id, Some(&entry.path));
    match sla_path {
        Some(path) => match load_construct_templates_from_sla(&path) {
            Ok(library) => SpaceMap::from_sla_spaces(&library.spaces),
            Err(_) => SpaceMap::empty(),
        },
        None => SpaceMap::empty(),
    }
}

fn find_packaged_sla(
    _entry_id: &str,
    entry_spec_path: Option<&std::path::Path>,
) -> Option<PathBuf> {
    let spec_path = entry_spec_path?;
    packaged_sla_for_entry_spec(spec_path).ok().flatten()
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
    let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)?;
    let frontend = frontends
        .first()
        .ok_or_else(|| anyhow!("no executable SLEIGH frontend candidates"))?;
    let space_map = load_space_map(frontend);
    let frontend_load_sec = frontend_load_started.elapsed().as_secs_f64();

    let mut instructions = Vec::new();
    let address_state = frontend.normalize_low_bit_code_address(args.address);
    let mut current = address_state.address;
    let initial_context_override = address_state.context_override;
    let mut pending_context_overrides = BTreeMap::<u64, PackedContextOverride>::new();
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
        let context_override = match (
            initial_context_override,
            pending_context_overrides.get(&current).copied(),
        ) {
            (Some(base), Some(pending)) => Some(merge_context_overrides(base, pending)),
            (Some(base), None) => Some(base),
            (None, Some(pending)) => Some(pending),
            (None, None) => None,
        };
        let mut decoded = None;
        let mut lifted = None;
        for candidate in &frontends {
            let candidate_decoded = candidate
                .decode_instruction_with_context_override(bytes, current, context_override)
                .map(|instruction| vec![instruction]);
            match candidate.decode_and_lift_with_context_override(bytes, current, context_override)
            {
                Ok(result) => {
                    decoded = candidate_decoded.ok();
                    lifted = Some(Ok(result));
                    break;
                }
                Err(err) => {
                    if lifted.is_none() {
                        lifted = Some(Err(err));
                    }
                }
            }
        }
        decode_lift_sec += decode_lift_started.elapsed().as_secs_f64();
        match lifted.expect("at least one frontend candidate was attempted") {
            Ok((ops, len, details)) => {
                for (target_addr, word_index, mask, value) in &details.pending_context_commits {
                    let entry = pending_context_overrides.entry(*target_addr).or_default();
                    entry.merge_commit_word(*word_index, *mask, *value)?;
                }
                instructions.push(InstructionReport {
                    address: current,
                    status: "ok".to_string(),
                    error: None,
                    decoded: decoded.and_then(|mut window| window.pop()),
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
                current = current.checked_add(len).ok_or_else(|| {
                    anyhow!("instruction address overflow after 0x{current:x} length {len}")
                })?;
            }
            Err(err) => {
                instructions.push(InstructionReport {
                    address: current,
                    status: "error".to_string(),
                    error: Some(err.to_string()),
                    decoded: decoded.and_then(|mut window| window.pop()),
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
        decode_start_address: address_state.address,
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
