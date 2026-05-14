use fission_decompiler::{PcodeFunction, PcodeOp, Varnode};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{
    DecodeContract, DecodeMemoryContext, DecodeStopReason, RuntimeSleighFrontend,
};
use std::io::{self, Write};

pub(super) fn emit_raw_pcode(
    binary: &LoadedBinary,
    addr: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect: bool,
    json: bool,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let frontend = runtime_frontend_for_binary(binary)?;
    let address_state = frontend.normalize_low_bit_code_address(addr);
    let decode_addr = address_state.address;
    let bytes = binary.view_bytes(decode_addr, max_bytes).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("unable to read bytes at 0x{decode_addr:x}"),
        )
    })?;
    let contract = if continue_past_indirect {
        DecodeContract::decomp_function(instruction_limit)
    } else {
        DecodeContract::strict_function(instruction_limit)
    };
    let memory_context = decode_memory_context(binary, decode_addr);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            contract,
            &memory_context,
            address_state.context_override,
        )
        .map_err(to_io_error)?;

    if json {
        let result = serde_json::json!({
            "entry_address": format!("0x{:x}", addr),
            "decode_address": format!("0x{:x}", decode_addr),
            "decoded_instructions": lifted.decoded_instructions,
            "stop_reason": decode_stop_reason_label(lifted.stop_reason),
            "template_source_counts": lifted.template_source_counts,
            "pcode": lifted.function,
        });
        let json_output = serde_json::to_string_pretty(&result).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {e}"),
            )
        })?;
        writeln!(stdout, "{json_output}")?;
    } else {
        write!(
            stdout,
            "{}",
            render_pcode_text(
                addr,
                decode_addr,
                lifted.decoded_instructions,
                lifted.stop_reason,
                &lifted.function,
            )
        )?;
    }
    Ok(())
}

fn runtime_frontend_for_binary(binary: &LoadedBinary) -> io::Result<RuntimeSleighFrontend> {
    let load_spec = binary.load_spec().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("missing Ghidra load spec for '{}'", binary.path),
        )
    })?;
    RuntimeSleighFrontend::new_for_load_spec(load_spec).map_err(to_io_error)
}

fn decode_memory_context(binary: &LoadedBinary, entry_address: u64) -> DecodeMemoryContext {
    let inner = binary.inner();
    let mut relative_address_bases = Vec::new();
    for section in &inner.sections {
        let start = section.virtual_address;
        let end = start.saturating_add(section.virtual_size);
        if entry_address >= start && entry_address < end && !relative_address_bases.contains(&start)
        {
            relative_address_bases.push(start);
        }
    }
    if inner.image_base != 0 && !relative_address_bases.contains(&inner.image_base) {
        relative_address_bases.push(inner.image_base);
    }
    DecodeMemoryContext {
        relative_address_bases,
    }
}

fn to_io_error<E>(err: E) -> io::Error
where
    E: std::fmt::Display,
{
    io::Error::new(io::ErrorKind::Other, err.to_string())
}

fn render_pcode_text(
    entry_address: u64,
    decode_address: u64,
    decoded_instructions: usize,
    stop_reason: DecodeStopReason,
    pcode: &PcodeFunction,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// raw p-code entry=0x{entry_address:x} decode=0x{decode_address:x} instructions={decoded_instructions} stop={}\n",
        decode_stop_reason_label(stop_reason)
    ));
    for block in &pcode.blocks {
        out.push_str(&format!(
            "block_{} @ 0x{:x}\n",
            block.index, block.start_address
        ));
        for op in &block.ops {
            out.push_str(&format!("  {}\n", format_pcode_op(op)));
        }
    }
    out
}

fn format_pcode_op(op: &PcodeOp) -> String {
    let out_vn = op
        .output
        .as_ref()
        .map(format_varnode)
        .unwrap_or_else(|| "-".to_string());
    let in_vn = op
        .inputs
        .iter()
        .map(format_varnode)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[{:04}] 0x{:x} {:?}  {} <- {}",
        op.seq_num, op.address, op.opcode, out_vn, in_vn
    )
}

fn format_varnode(vn: &Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{})", vn.constant_val as u64, vn.size)
    } else {
        format!(
            "v(space={},off=0x{:x},size={})",
            vn.space_id, vn.offset, vn.size
        )
    }
}

fn decode_stop_reason_label(reason: DecodeStopReason) -> &'static str {
    match reason {
        DecodeStopReason::TerminalControlFlow => "terminal_control_flow",
        DecodeStopReason::InputExhausted => "input_exhausted",
        DecodeStopReason::InstructionLimit => "instruction_limit",
    }
}
