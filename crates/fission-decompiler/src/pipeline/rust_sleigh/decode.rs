use crate::{PcodeFunction, Varnode};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, DecodeMemoryContext, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;

#[derive(Debug, Clone)]
pub(crate) struct DecodeDiag {
    pub attempts: usize,
    pub stop_reason: String,
    pub template_source_counts: std::collections::BTreeMap<String, usize>,
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

fn decode_memory_context(
    binary: &LoadedBinary,
    entry_address: u64,
    max_bytes: usize,
) -> DecodeMemoryContext {
    decode_memory_context_for(binary, entry_address, max_bytes)
}

pub(crate) fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<
    (
        PcodeFunction,
        DecodeDiag,
        std::collections::HashMap<u32, String>,
    ),
    DecodeFailure,
> {
    let load_spec = binary.load_spec().ok_or_else(|| DecodeFailure {
        message: format!(
            "rust_sleigh: missing Ghidra load spec for '{}'",
            binary.path
        ),
        diag: DecodeDiag {
            attempts: 0,
            stop_reason: "missing_load_spec".into(),
            template_source_counts: Default::default(),
        },
    })?;

    let lifters =
        RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec).map_err(|e| {
            DecodeFailure {
                message: format!("rust_sleigh: {e:#}"),
                diag: DecodeDiag {
                    attempts: 0,
                    stop_reason: "lifter_init_failed".into(),
                    template_source_counts: Default::default(),
                },
            }
        })?;
    let lifter = lifters.first().ok_or_else(|| DecodeFailure {
        message: format!(
            "rust_sleigh: no executable SLEIGH frontend candidates for '{}'",
            binary.path
        ),
        diag: DecodeDiag {
            attempts: 0,
            stop_reason: "lifter_init_failed".into(),
            template_source_counts: Default::default(),
        },
    })?;
    let userops = lifter
        .compiled_frontend()
        .map(|c| {
            c.userops
                .iter()
                .map(|(&k, v)| (k, v.clone()))
                .collect::<std::collections::HashMap<u32, String>>()
        })
        .unwrap_or_default();
    let address_state = lifter.normalize_low_bit_code_address(entry_address);
    let decode_entry_address = address_state.address;
    let initial_context_override = address_state.context_override;
    let bytes = binary
        .view_bytes(decode_entry_address, max_bytes)
        .ok_or_else(|| DecodeFailure {
            message: format!(
                "rust_sleigh: unable to read bytes at 0x{decode_entry_address:x} for {name}"
            ),
            diag: DecodeDiag {
                attempts: 0,
                stop_reason: "view_bytes_unavailable".into(),
                template_source_counts: Default::default(),
            },
        })?;

    let lift_contract = if continue_past_indirect_branch {
        DecodeContract::decomp_function(instruction_limit)
    } else {
        DecodeContract::strict_function(instruction_limit)
    };
    let memory_context = decode_memory_context(binary, decode_entry_address, max_bytes);
    let result = lifter.lift_raw_pcode_function_with_context_and_memory_context(
        bytes,
        decode_entry_address,
        lift_contract,
        &memory_context,
        initial_context_override,
    );
    match result {
        Ok(lifted) => {
            let template_source_counts = lifted.template_source_counts.clone();
            Ok((
                lifted.function,
                DecodeDiag {
                    attempts: 1,
                    stop_reason: "success_first_lift".into(),
                    template_source_counts,
                },
                userops.clone(),
            ))
        }
        Err(first_err) => {
            if retry_on_decode_error {
                for variant_lifter in lifters.iter().skip(1) {
                    let variant_address_state =
                        variant_lifter.normalize_low_bit_code_address(entry_address);
                    let variant_decode_entry_address = variant_address_state.address;
                    let Some(variant_bytes) =
                        binary.view_bytes(variant_decode_entry_address, max_bytes)
                    else {
                        continue;
                    };
                    let variant_memory_context =
                        decode_memory_context(binary, variant_decode_entry_address, max_bytes);
                    if let Ok(retry) = variant_lifter
                        .lift_raw_pcode_function_with_context_and_memory_context(
                            variant_bytes,
                            variant_decode_entry_address,
                            lift_contract,
                            &variant_memory_context,
                            variant_address_state.context_override,
                        )
                    {
                        let template_source_counts = retry.template_source_counts.clone();
                        return Ok((
                            retry.function,
                            DecodeDiag {
                                attempts: 2,
                                stop_reason: format!(
                                    "success_after_sibling_language_retry:{}",
                                    variant_lifter.entry().entry_id
                                ),
                                template_source_counts,
                            },
                            userops.clone(),
                        ));
                    }
                }

                if continue_past_indirect_branch {
                    if let Ok(retry) = lifter
                        .lift_raw_pcode_function_with_context_and_memory_context(
                            &bytes,
                            decode_entry_address,
                            DecodeContract::strict_function(instruction_limit),
                            &memory_context,
                            initial_context_override,
                        )
                    {
                        let template_source_counts = retry.template_source_counts.clone();
                        return Ok((
                            retry.function,
                            DecodeDiag {
                                attempts: 2,
                                stop_reason: "success_after_strict_indirect_retry".into(),
                                template_source_counts,
                            },
                            userops.clone(),
                        ));
                    }
                }

                let err_str = format!("{first_err:#}");
                if let Some(safe) =
                    extract_safe_bytes_from_decode_error(&err_str, decode_entry_address)
                {
                    if safe > 0 && safe < bytes.len() {
                        if let Ok(retry) = lifter
                            .lift_raw_pcode_function_with_context_and_memory_context(
                                &bytes[..safe],
                                decode_entry_address,
                                lift_contract,
                                &memory_context,
                                initial_context_override,
                            )
                        {
                            let template_source_counts = retry.template_source_counts.clone();
                            return Ok((
                                retry.function,
                                DecodeDiag {
                                    attempts: 2,
                                    stop_reason: "success_after_truncated_retry".into(),
                                    template_source_counts,
                                },
                                userops.clone(),
                            ));
                        }
                        return Err(DecodeFailure {
                            message: format!(
                                "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err:#}"
                            ),
                            diag: DecodeDiag {
                                attempts: 2,
                                stop_reason: "lift_failed_after_truncated_retry".into(),
                                template_source_counts: Default::default(),
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
                    template_source_counts: Default::default(),
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::LoadedBinary;
    use fission_pcode::PcodeOpcode;
    use std::path::Path;

    #[test]
    fn decomp_lift_does_not_fallthrough_into_jump_table_data() {
        let fixture =
            Path::new("../../benchmark/binary/ARM4_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM4_be control_flow fixture");
        let (pcode, diag, _userops) =
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
        let fixture =
            Path::new("../../benchmark/binary/ARM5_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM5_be control_flow fixture");
        let (pcode, diag, _userops) =
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
        let branchind_block = pcode
            .blocks
            .iter()
            .find(|block| {
                block
                    .ops
                    .last()
                    .is_some_and(|op| op.opcode == PcodeOpcode::BranchInd)
            })
            .expect("decoded p-code should retain branch-indirect block");
        let successor_starts = branchind_block
            .successors
            .iter()
            .filter_map(|succ_idx| pcode.blocks.get(*succ_idx as usize))
            .map(|block| block.start_address)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            successor_starts,
            [0x100034, 0x10003c, 0x100048, 0x100050]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
        );
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
