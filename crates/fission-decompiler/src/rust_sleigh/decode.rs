use crate::{PcodeFunction, Varnode};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, DecodeMemoryContext, RuntimeSleighFrontend};

#[derive(Debug, Clone)]
pub(crate) struct DecodeDiag {
    pub attempts: usize,
    pub stop_reason: String,
}

#[derive(Debug)]
pub(crate) struct DecodeFailure {
    pub message: String,
    pub diag: DecodeDiag,
}

fn extract_safe_bytes_from_decode_error(err: &str, func_addr: u64) -> Option<usize> {
    let marker = "decode failed at 0x";
    let idx = err.find(marker)?;
    let hex_start = idx + marker.len();
    let hex_end = err[hex_start..]
        .find(|c: char| !c.is_ascii_hexdigit())
        .map(|i| hex_start + i)
        .unwrap_or(err.len());
    let fail_addr = u64::from_str_radix(&err[hex_start..hex_end], 16).ok()?;
    let safe = fail_addr.checked_sub(func_addr)? as usize;
    if safe == 0 { None } else { Some(safe) }
}

pub(crate) fn pcode_op_count(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|b| b.ops.len()).sum()
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

pub(crate) fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<(PcodeFunction, DecodeDiag), DecodeFailure> {
    let bytes = binary
        .view_bytes(entry_address, max_bytes)
        .ok_or_else(|| DecodeFailure {
            message: format!("rust_sleigh: unable to read bytes at 0x{entry_address:x} for {name}"),
            diag: DecodeDiag {
                attempts: 0,
                stop_reason: "view_bytes_unavailable".into(),
            },
        })?;

    let load_spec = binary.load_spec().ok_or_else(|| DecodeFailure {
        message: format!(
            "rust_sleigh: missing Ghidra load spec for '{}'",
            binary.path
        ),
        diag: DecodeDiag {
            attempts: 0,
            stop_reason: "missing_load_spec".into(),
        },
    })?;

    let lifter =
        RuntimeSleighFrontend::new_for_load_spec(load_spec).map_err(|e| DecodeFailure {
            message: format!("rust_sleigh: {e:#}"),
            diag: DecodeDiag {
                attempts: 0,
                stop_reason: "lifter_init_failed".into(),
            },
        })?;

    let lift_contract = if continue_past_indirect_branch {
        DecodeContract::decomp_function(instruction_limit)
    } else {
        DecodeContract::strict_function(instruction_limit)
    };
    let memory_context = decode_memory_context(binary, entry_address);
    let result = lifter.lift_raw_pcode_function_with_decode_contract_and_memory_context(
        bytes,
        entry_address,
        lift_contract,
        &memory_context,
    );
    match result {
        Ok(lifted) => Ok((
            lifted.function,
            DecodeDiag {
                attempts: 1,
                stop_reason: "success_first_lift".into(),
            },
        )),
        Err(first_err) => {
            if retry_on_decode_error {
                if continue_past_indirect_branch {
                    if let Ok(retry) = lifter.lift_raw_pcode_function_with_decode_contract(
                        &bytes,
                        entry_address,
                        DecodeContract::strict_function(instruction_limit),
                    ) {
                        return Ok((
                            retry.function,
                            DecodeDiag {
                                attempts: 2,
                                stop_reason: "success_after_strict_indirect_retry".into(),
                            },
                        ));
                    }
                }

                let err_str = format!("{first_err:#}");
                if let Some(safe) = extract_safe_bytes_from_decode_error(&err_str, entry_address) {
                    if safe > 0 && safe < bytes.len() {
                        if let Ok(retry) = lifter
                            .lift_raw_pcode_function_with_decode_contract_and_memory_context(
                                &bytes[..safe],
                                entry_address,
                                lift_contract,
                                &memory_context,
                            )
                        {
                            return Ok((
                                retry.function,
                                DecodeDiag {
                                    attempts: 2,
                                    stop_reason: "success_after_truncated_retry".into(),
                                },
                            ));
                        }
                        return Err(DecodeFailure {
                            message: format!(
                                "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err:#}"
                            ),
                            diag: DecodeDiag {
                                attempts: 2,
                                stop_reason: "lift_failed_after_truncated_retry".into(),
                            },
                        });
                    }
                }
            }
            Err(DecodeFailure {
                message: format!(
                    "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err:#}"
                ),
                diag: DecodeDiag {
                    attempts: 1,
                    stop_reason: "lift_failed".into(),
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::LoadedBinary;
    use fission_sleigh::compiler::discovery;
    use std::path::Path;

    #[test]
    fn decomp_lift_does_not_fallthrough_into_jump_table_data() {
        if !discovery::ghidra_packaged_sla_available() {
            eprintln!("skip: packaged Ghidra .sla not available for ARM strict retry check");
            return;
        }

        let fixture =
            Path::new("../../benchmark/binary/ARM4_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM4_be control_flow fixture");
        let (pcode, diag) =
            decode_rust_sleigh_pcode(&binary, "run_control_flow", 0x100150, 616, 512, true, true)
                .expect("indirect branch should not fall through into inline jump-table data");

        assert_eq!(diag.attempts, 1);
        assert_eq!(diag.stop_reason, "success_first_lift");
        assert!(!pcode.blocks.is_empty());
        assert!(
            !pcode
                .blocks
                .iter()
                .any(|block| (0x1002a8..0x1002c8).contains(&block.start_address)),
            "{:?}",
            pcode
                .blocks
                .iter()
                .map(|block| block.start_address)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn conditional_return_keeps_fallthrough_without_decoding_jump_table_data() {
        if !discovery::ghidra_packaged_sla_available() {
            eprintln!("skip: packaged Ghidra .sla not available for ARM conditional return check");
            return;
        }

        let fixture =
            Path::new("../../benchmark/binary/ARM5_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM5_be control_flow fixture");
        let (pcode, diag) =
            decode_rust_sleigh_pcode(&binary, "test_switch", 0x100000, 92, 512, true, true)
                .expect("conditional bx return should not end the function early");

        assert_eq!(diag.attempts, 1);
        assert!(
            pcode
                .blocks
                .iter()
                .any(|block| block.start_address == 0x10001c)
        );
        for expected_case in [0x100034, 0x10003c, 0x100048, 0x100050] {
            assert!(
                pcode
                    .blocks
                    .iter()
                    .any(|block| block.start_address == expected_case),
                "missing decoded switch case 0x{expected_case:x}: {:?}",
                pcode
                    .blocks
                    .iter()
                    .map(|block| block.start_address)
                    .collect::<Vec<_>>()
            );
        }
        assert!(
            !pcode
                .blocks
                .iter()
                .any(|block| (0x100024..0x100034).contains(&block.start_address)),
            "{:?}",
            pcode
                .blocks
                .iter()
                .map(|block| block.start_address)
                .collect::<Vec<_>>()
        );
    }
}

fn format_varnode_for_pcode(vn: &Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{})", vn.constant_val as u64, vn.size)
    } else {
        format!(
            "v(space={},off=0x{:x},size={})",
            vn.space_id, vn.offset, vn.size
        )
    }
}

pub(crate) fn render_pcode_text(name: &str, pcode: &PcodeFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("// rust_sleigh direct pcode output: {name}\n"));
    for block in &pcode.blocks {
        out.push_str(&format!(
            "block_{} @ 0x{:x}\n",
            block.index, block.start_address
        ));
        for op in &block.ops {
            let out_vn = op
                .output
                .as_ref()
                .map(format_varnode_for_pcode)
                .unwrap_or_else(|| "-".to_string());
            let in_vn = op
                .inputs
                .iter()
                .map(format_varnode_for_pcode)
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  [{:04}] 0x{:x} {:?}  {} <- {}\n",
                op.seq_num, op.address, op.opcode, out_vn, in_vn
            ));
        }
    }
    out
}
