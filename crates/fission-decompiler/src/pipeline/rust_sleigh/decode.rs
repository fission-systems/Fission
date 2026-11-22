use crate::{PcodeFunction, Varnode};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, DecodeMemoryContext, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;
use fission_static::analysis::function_discovery::decode_context_for_address;

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
    cached_decoded: Option<std::sync::Arc<fission_sleigh::runtime::DecodedPcodeFunction>>,
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
    // A Thumb-only image (Cortex-M/ARMv7-M) is a *fact*, not a guess: it has
    // no ARM-mode execution at all, so every address in it decodes as Thumb
    // whatever its bit 0 says. `decode_context_for_address` is the one
    // primitive that owns that -- `raw-pcode`, `disasm`, `similar` and
    // function discovery all consult it, and this path did not, so it alone
    // fell through to the per-address heuristic below and lifted ARM over
    // Thumb bytes on exactly the functions the heuristic has no pattern for
    // (crazyflie `cf2.elf`'s leaf `invSqrt` opens `sub sp,#0x18`, no `push`,
    // and read as `stc p0,cr11,[sp,#536]`). The heuristic still owns genuine
    // interworking images, where this returns `None`.
    let initial_context_override = decode_context_for_address(
        binary,
        lifter,
        entry_address,
        address_state.context_override,
    );
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
    // A cache hit is only offered by the caller when its own contract kind
    // matches (`decomp_function`, i.e. `continue_past_indirect_branch`) --
    // see `FactStore::get_cached_decoded_function`'s doc comment for why
    // that's required, not just a budget check. Skips the (expensive) lift
    // call entirely; everything downstream reads the same `DecodedPcodeFunction`
    // shape either way.
    // A cached decode is reused only when there is nothing left to resolve in
    // it. The cache is filled by the FID sweep, which decodes under the same
    // contract and therefore stops at the same computed jumps -- so taking it
    // unconditionally handed the decompiler a function that had already given
    // up on every `switch`, and the fixed point below never got to run.
    // Kept even when the bypass below rejects it: bypassing means "there is
    // something left to resolve here", not "this decode is unusable". If the
    // fresh lift then fails outright, the cached decode is still a real decode
    // of the function -- 14 functions lost their decompilation on a corpus
    // sweep because the bypass threw it away and nothing caught the fall.
    let cached_fallback = cached_decoded.clone();
    let cached_decoded = cached_decoded.filter(|cached| {
        !continue_past_indirect_branch
            || fission_sleigh::runtime::resolve_indirect_branch_targets(
                &cached.function,
                decode_entry_address,
                bytes,
                &memory_context,
                binary.is_little_endian(),
            )
            .is_empty()
    });
    if let Some(cached) = cached_decoded.filter(|_| continue_past_indirect_branch) {
        let template_source_counts = cached.template_source_counts.clone();
        return Ok((
            cached.function.clone(),
            DecodeDiag {
                attempts: 0,
                stop_reason: "success_cached_fid_decode".into(),
                template_source_counts,
            },
            userops,
        ));
    }
    // Thumb-only targets (Cortex-M/ARMv7-M) can NEVER execute ARM-mode
    // instructions, but a bare even address (as DecBench's eval kit and
    // other external callers report, having normalized away the ABI's
    // Thumb-bit marker -- see `low_bit_code_mode_override`'s doc comment)
    // is indistinguishable from a genuine ARM-mode target at this point.
    // The retry cascade below only reaches the Thumb-forced fallback
    // *after* the default-mode attempt visibly fails -- but misreading
    // Thumb bytes as ARM rarely fails outright (ARM's 32-bit encoding
    // space is dense enough that almost any bit pattern decodes to
    // *something*), it just silently produces wrong code (e.g. a
    // plausible-looking `b <address>` whose target is actually garbage).
    // Resolve the ambiguity per-*address*, not per-binary: a whole-binary
    // "odd entry point" signal misses genuinely-Thumb functions living in a
    // binary whose entry point happens to be even (observed on crazyflie's
    // cf2.elf). `should_prefer_thumb_decode` mirrors angr/archinfo's
    // no-cross-reference-needed heuristic (4-byte-alignment fact, then
    // known Thumb prologue byte patterns, then the entry-point parity as a
    // last-resort weak prior) -- see its doc comment for why this fits
    // Fission's single-address decode model better than Ghidra's
    // full-binary constant-propagation approach.
    let base_memory_context = memory_context.clone();
    let mut memory_context = memory_context;
    let thumb_preferred = super::arm_thumb_heuristic::should_prefer_thumb_decode(
        decode_entry_address,
        bytes,
        binary.entry_point,
        &binary.hash,
    );
    let mut active_context_override = initial_context_override;
    let mut first_lift_stop_reason = "success_first_lift";
    let mut initial_override_lifted = None;
    if thumb_preferred
        && initial_context_override.is_none()
        && let Some(forced_override) = lifter.low_bit_code_mode_override()
        && let Ok(thumb_first) = lifter.lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_entry_address,
            lift_contract,
            &memory_context,
            Some(forced_override),
        )
    {
        active_context_override = Some(forced_override);
        first_lift_stop_reason = "success_thumb_preferred_entry_hint";
        initial_override_lifted = Some(thumb_first);
    }

    let (result, last_good, discovered_edges) = run_indirect_branch_fixpoint(
        lifter,
        binary.is_little_endian(),
        bytes,
        decode_entry_address,
        lift_contract,
        continue_past_indirect_branch,
        active_context_override,
        &mut memory_context,
        initial_override_lifted,
    );

    match result {
        Ok(lifted) => {
            let template_source_counts = lifted.template_source_counts.clone();
            super::arm_thumb_heuristic::record_direct_call_targets(
                &binary.hash,
                active_context_override.is_some(),
                &lifted.instructions,
            );
            Ok((
                lifted.function,
                DecodeDiag {
                    attempts: 1,
                    stop_reason: first_lift_stop_reason.into(),
                    template_source_counts,
                },
                userops.clone(),
            ))
        }
        Err(first_err) => {
            let mut truncated_retry_failed = false;
            if retry_on_decode_error {
                if last_good.is_none() {
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
                        if let Ok(mut retry) = variant_lifter
                            .lift_raw_pcode_function_with_context_and_memory_context(
                                variant_bytes,
                                variant_decode_entry_address,
                                lift_contract,
                                &variant_memory_context,
                                variant_address_state.context_override,
                            )
                        {
                            if !discovered_edges.is_empty() {
                                fission_sleigh::runtime::attach_inferred_indirect_edges(
                                    &mut retry.function,
                                    &discovered_edges,
                                );
                            }
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
                        if let Ok(mut retry) = lifter
                            .lift_raw_pcode_function_with_context_and_memory_context(
                                bytes,
                                decode_entry_address,
                                DecodeContract::strict_function(instruction_limit),
                                &base_memory_context,
                                active_context_override,
                            )
                        {
                            if !discovered_edges.is_empty() {
                                fission_sleigh::runtime::attach_inferred_indirect_edges(
                                    &mut retry.function,
                                    &discovered_edges,
                                );
                            }
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
                }

                let err_str = first_err.clone();
                if let Some(safe) =
                    extract_safe_bytes_from_decode_error(&err_str, decode_entry_address)
                {
                    if safe > 0 && safe < bytes.len() {
                        if let Ok(mut retry) = lifter
                            .lift_raw_pcode_function_with_context_and_memory_context(
                                &bytes[..safe],
                                decode_entry_address,
                                lift_contract,
                                &memory_context,
                                active_context_override,
                            )
                        {
                            if !discovered_edges.is_empty() {
                                fission_sleigh::runtime::attach_inferred_indirect_edges(
                                    &mut retry.function,
                                    &discovered_edges,
                                );
                            }
                            super::arm_thumb_heuristic::record_direct_call_targets(
                                &binary.hash,
                                active_context_override.is_some(),
                                &retry.instructions,
                            );
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
                        truncated_retry_failed = true;
                    }
                }

                if let Some(mut fallback) = last_good {
                    super::arm_thumb_heuristic::record_direct_call_targets(
                        &binary.hash,
                        active_context_override.is_some(),
                        &fallback.instructions,
                    );
                    if !discovered_edges.is_empty() {
                        fission_sleigh::runtime::attach_inferred_indirect_edges(
                            &mut fallback.function,
                            &discovered_edges,
                        );
                    }
                    let template_source_counts = fallback.template_source_counts.clone();
                    return Ok((
                        fallback.function,
                        DecodeDiag {
                            attempts: 2,
                            stop_reason: "success_from_decode_fixpoint_fallback".into(),
                            template_source_counts,
                        },
                        userops.clone(),
                    ));
                }

                // `entry_address` was even, so `normalize_low_bit_code_address`
                // trusted its bit 0 and assumed the CPU's default (non-Thumb)
                // code mode -- correct for a real ARM/Thumb-interworking
                // function pointer, but wrong for a Thumb-only target
                // (Cortex-M/ARMv7-M) given a bare address with the ABI's
                // Thumb marker already normalized away (e.g. by an external
                // caller reporting addresses in stripped form). Only try this
                // once every other retry has failed, and only when we didn't
                // already decode in forced low-bit-code mode.
                if active_context_override.is_none()
                    && let Some(forced_override) = lifter.low_bit_code_mode_override()
                {
                    let mut forced_memory_context = base_memory_context.clone();
                    let (forced_result, forced_last_good, _) = run_indirect_branch_fixpoint(
                        lifter,
                        binary.is_little_endian(),
                        bytes,
                        decode_entry_address,
                        lift_contract,
                        continue_past_indirect_branch,
                        Some(forced_override),
                        &mut forced_memory_context,
                        None,
                    );
                    if let Some(retry) = forced_result.ok().or(forced_last_good) {
                        super::arm_thumb_heuristic::record_direct_call_targets(
                            &binary.hash,
                            true,
                            &retry.instructions,
                        );
                        let template_source_counts = retry.template_source_counts.clone();
                        return Ok((
                            retry.function,
                            DecodeDiag {
                                attempts: 2,
                                stop_reason: "success_after_forced_low_bit_code_mode_retry".into(),
                                template_source_counts,
                            },
                            userops.clone(),
                        ));
                    }
                }
            } else if let Some(mut fallback) = last_good {
                super::arm_thumb_heuristic::record_direct_call_targets(
                    &binary.hash,
                    active_context_override.is_some(),
                    &fallback.instructions,
                );
                if !discovered_edges.is_empty() {
                    fission_sleigh::runtime::attach_inferred_indirect_edges(
                        &mut fallback.function,
                        &discovered_edges,
                    );
                }
                let template_source_counts = fallback.template_source_counts.clone();
                return Ok((
                    fallback.function,
                    DecodeDiag {
                        attempts: 2,
                        stop_reason: "success_from_decode_fixpoint_fallback".into(),
                        template_source_counts,
                    },
                    userops.clone(),
                ));
            }
            // Every retry failed. If the decode fixed point had a round that
            // lifted before a later one failed, that round is a real decode of
            // this function; returning it beats returning an error, and it is
            // exactly the function this call would have produced had the loop
            // stopped one round earlier.
            if let Some(mut fallback) = cached_fallback.map(|cached| (*cached).clone()) {
                if !discovered_edges.is_empty() {
                    fission_sleigh::runtime::attach_inferred_indirect_edges(
                        &mut fallback.function,
                        &discovered_edges,
                    );
                }
                let template_source_counts = fallback.template_source_counts.clone();
                return Ok((
                    fallback.function,
                    DecodeDiag {
                        attempts: 2,
                        stop_reason: "success_from_decode_fixpoint_fallback".into(),
                        template_source_counts,
                    },
                    userops.clone(),
                ));
            }
            Err(DecodeFailure {
                message: format!(
                    "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err}"
                ),
                diag: DecodeDiag {
                    attempts: if truncated_retry_failed { 2 } else { 1 },
                    stop_reason: if truncated_retry_failed {
                        "lift_failed_after_truncated_retry"
                    } else {
                        "lift_failed"
                    }
                    .into(),
                    template_source_counts: Default::default(),
                },
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_indirect_branch_fixpoint(
    lifter: &RuntimeSleighFrontend,
    little_endian: bool,
    bytes: &[u8],
    decode_entry_address: u64,
    lift_contract: DecodeContract,
    continue_past_indirect_branch: bool,
    context_override: Option<fission_sleigh::runtime::PackedContextOverride>,
    memory_context: &mut DecodeMemoryContext,
    initial_override_lifted: Option<fission_sleigh::runtime::DecodedPcodeFunction>,
) -> (
    Result<fission_sleigh::runtime::DecodedPcodeFunction, String>,
    Option<fission_sleigh::runtime::DecodedPcodeFunction>,
    std::collections::BTreeMap<u64, Vec<u64>>,
) {
    // ── Decode/analyse fixed point ────────────────────────────────────────
    // Recursive descent cannot enumerate the successors of a computed jump, so
    // a `switch` dispatch leaves every case unreachable and the function is
    // decoded only as far as its dispatcher. `bzip2`'s `BZ2_decompress`
    // reached 232 of ~6,000 instructions that way and rendered as
    // straight-line C; the same shape accounts for a third of our whole
    // distance to `kuna` on DecBench, across 769 functions.
    //
    // Only an analysis over *decoded* code can name those successors, and that
    // analysis needs the decode that needs its answer. So run them alternately:
    // lift what is reachable, resolve the indirect branches in the result, feed
    // the new targets back as decode entries, and lift again until nothing new
    // appears. `additional_decode_entries` already seeds both the decode
    // worklist and the reachability pass, which is what makes the feedback a
    // few lines rather than a new pipeline.
    let mut last_good: Option<fission_sleigh::runtime::DecodedPcodeFunction> = None;
    let mut discovered_edges: std::collections::BTreeMap<u64, Vec<u64>> = Default::default();
    let mut result = match initial_override_lifted {
        Some(lifted) => Ok(lifted),
        None => lifter
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                decode_entry_address,
                lift_contract,
                memory_context,
                context_override,
            )
            .map_err(|err| format!("{err:#}")),
    };
    if continue_past_indirect_branch {
        last_good = result.as_ref().ok().cloned();
        // Each round can only add entries, and a round that adds none stops
        // the loop, so this bound is a guard against a pathological input
        // rather than the usual exit: a `switch` inside a `switch` converges
        // in three.
        const MAX_DECODE_ROUNDS: usize = 8;
        for _ in 0..MAX_DECODE_ROUNDS {
            let Ok(lifted) = result.as_ref() else {
                break;
            };
            let discovered = fission_sleigh::runtime::resolve_indirect_branch_targets(
                &lifted.function,
                decode_entry_address,
                bytes,
                memory_context,
                little_endian,
            );
            if std::env::var_os("FISSION_JT_TRACE").is_some() {
                eprintln!(
                    "[JT] entry=0x{decode_entry_address:x} round_blocks={} dispatches={} targets={}",
                    lifted.function.blocks.len(),
                    discovered.len(),
                    discovered.values().map(Vec::len).sum::<usize>()
                );
            }
            let fresh: Vec<u64> = discovered
                .values()
                .flatten()
                .copied()
                .filter(|target| !memory_context.additional_decode_entries.contains(target))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for (source, targets) in discovered {
                discovered_edges.entry(source).or_default().extend(targets);
            }
            if fresh.is_empty() {
                break;
            }
            memory_context.additional_decode_entries.extend(fresh);
            let next = lifter
                .lift_raw_pcode_function_with_context_and_memory_context(
                    bytes,
                    decode_entry_address,
                    lift_contract,
                    memory_context,
                    context_override,
                )
                .map_err(|err| format!("{err:#}"));
            if let Ok(better) = &next {
                last_good = Some(better.clone());
            }
            result = next;
        }
    }
    if !discovered_edges.is_empty() {
        if let Ok(lifted) = &mut result {
            fission_sleigh::runtime::attach_inferred_indirect_edges(
                &mut lifted.function,
                &discovered_edges,
            );
        }
        if let Some(good) = &mut last_good {
            fission_sleigh::runtime::attach_inferred_indirect_edges(
                &mut good.function,
                &discovered_edges,
            );
        }
    }
    (result, last_good, discovered_edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::LoadedBinary;
    use fission_pcode::PcodeOpcode;
    use std::path::Path;

    #[test]
    fn decomp_lift_does_not_fallthrough_into_jump_table_data() {
        let fixture = Path::new("../../benchmark")
            .join("binary/ARM4_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM4_be control_flow fixture");
        let (pcode, diag, _userops) = decode_rust_sleigh_pcode(
            &binary,
            "run_control_flow",
            0x100150,
            616,
            512,
            true,
            true,
            None,
        )
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
        let fixture = Path::new("../../benchmark")
            .join("binary/ARM5_be/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: benchmark fixture not found: {}", fixture.display());
            return;
        }

        let binary = LoadedBinary::from_file(fixture).expect("load ARM5_be control_flow fixture");
        let (pcode, diag, _userops) =
            decode_rust_sleigh_pcode(&binary, "test_switch", 0x100000, 92, 512, true, true, None)
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

#[cfg(test)]
mod thumb_only_image_tests {
    use super::*;
    use std::path::PathBuf;

    /// `testdata/armv7m_thumb_leaf.elf`: a Cortex-M image (vector table with
    /// an SRAM stack pointer and Thumb-marked handlers) whose entry is even,
    /// holding one leaf function that opens `sub sp,#0x18` -- no `push`, so
    /// no Thumb prologue pattern matches it, and 4-byte-aligned, so the
    /// alignment rule says nothing either. Read as ARM the same 12 bytes are
    /// three perfectly valid instructions (`andcs`, `ldrmi`, `bx lr`), so the
    /// misread raises no error at all; it just decompiles the wrong program.
    fn fixture() -> LoadedBinary {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fission-static/testdata/armv7m_thumb_leaf.elf");
        LoadedBinary::from_file(&path).expect("load ARMv7-M fixture")
    }

    #[test]
    fn a_thumb_only_image_decodes_thumb_at_an_even_address() {
        let binary = fixture();
        let (pcode, diag, _) =
            decode_rust_sleigh_pcode(&binary, "leaf", 0x0800_0020, 0x40, 64, true, true, None)
                .expect("decode");

        // Thumb decodes these 12 bytes as four 2-byte instructions at
        // 0x20/0x22/0x24/0x26 and stops at `bx lr`; ARM as three 4-byte ones
        // at 0x20/0x24/0x28. The instruction *grid* is the discriminator --
        // and both readings terminate cleanly, which is exactly why nothing
        // downstream can catch the wrong one.
        let mut addresses: Vec<u64> = pcode
            .blocks
            .iter()
            .flat_map(|b| b.ops.iter())
            .map(|op| op.address)
            .collect();
        addresses.dedup();
        assert_eq!(
            addresses,
            vec![0x0800_0020, 0x0800_0022, 0x0800_0024, 0x0800_0026],
            "expected a Thumb instruction grid (stop={})",
            diag.stop_reason
        );
    }

    /// A jump table whose second entry lands on bytes that do not decode.
    ///
    /// `.text` (0x1000):  `jmp [rdi*8 + 0x2000]`, `nop`, `ret` at 0x1008, and
    /// an invalid-in-64-bit opcode (`06`) at 0x1009.
    /// `.rodata` (0x2000): the table `{0x1008, 0x1009}`.
    ///
    /// The table lives in a read-only data section, so the in-lift reader
    /// (function bytes only) does not see it and round 0 lifts cleanly up to
    /// the dispatch. The post-decode resolver then finds both entries, round 1
    /// seeds them, and decoding 0x1009 fails. That is the error path after a
    /// good fixpoint round: the decode must not throw away what the fixed
    /// point learned by re-lifting under the strict contract with the base
    /// memory context (which knows neither the entries nor the edges), but
    /// keep the seeded case at 0x1008 wired as the dispatch's successor.
    #[test]
    fn decode_error_after_good_fixpoint_round_keeps_discovered_edges() {
        let load_spec = fission_core::architecture::BinaryLoadSpec {
            format: "ELF".to_string(),
            image_base: 0x1000,
            pair: fission_core::architecture::LanguageCompilerSpecPair {
                language_id: fission_core::architecture::GhidraLanguageId(
                    "x86:LE:64:default".to_string(),
                ),
                compiler_spec_id: fission_core::architecture::CompilerSpecId("gcc".to_string()),
            },
            preferred: true,
            source: "test".to_string(),
        };
        let mut data = vec![0x90u8; 0x30];
        // jmp qword ptr [rdi*8 + 0x2000]
        data[..7].copy_from_slice(&[0xff, 0x24, 0xfd, 0x00, 0x20, 0x00, 0x00]);
        data[8] = 0xc3; // 0x1008: ret
        data[9] = 0x06; // 0x1009: invalid in 64-bit mode
        data[0x20..0x28].copy_from_slice(&0x1008u64.to_le_bytes());
        data[0x28..0x30].copy_from_slice(&0x1009u64.to_le_bytes());
        let binary = fission_loader::loader::LoadedBinaryBuilder::new(
            "jt_decode_error_after_good_round.elf".to_string(),
            fission_loader::loader::DataBuffer::Heap(data),
        )
        .format("ELF")
        .is_64bit(true)
        .load_spec(load_spec)
        .add_section(fission_loader::loader::SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x1000,
            virtual_size: 0x10,
            file_offset: 0,
            file_size: 0x10,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_section(fission_loader::loader::SectionInfo {
            name: ".rodata".to_string(),
            virtual_address: 0x2000,
            virtual_size: 0x10,
            file_offset: 0x20,
            file_size: 0x10,
            is_executable: false,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("build jump-table fixture");

        // Preconditions, so the test cannot pass by never reaching the path:
        // round 0 lifts, and the resolver finds both table entries in it.
        let lifter = RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().expect("spec"))
            .expect("lifter");
        let bytes = binary.view_bytes(0x1000, 0x10).expect("bytes");
        let memory_context = decode_memory_context(&binary, 0x1000, 0x10);
        let round_zero = lifter
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                0x1000,
                DecodeContract::decomp_function(64),
                &memory_context,
                None,
            )
            .expect("round 0 lifts up to the dispatch");
        let discovered = fission_sleigh::runtime::resolve_indirect_branch_targets(
            &round_zero.function,
            0x1000,
            bytes,
            &memory_context,
            true,
        );
        assert_eq!(
            discovered,
            std::collections::BTreeMap::from([(0x1000, vec![0x1008, 0x1009])])
        );
        let mut seeded = memory_context.clone();
        seeded.additional_decode_entries.extend([0x1008, 0x1009]);
        assert!(
            lifter
                .lift_raw_pcode_function_with_context_and_memory_context(
                    bytes,
                    0x1000,
                    DecodeContract::decomp_function(64),
                    &seeded,
                    None,
                )
                .is_err(),
            "round 1 must fail on the invalid table entry"
        );

        let (pcode, diag, _) =
            decode_rust_sleigh_pcode(&binary, "dispatch", 0x1000, 0x10, 64, true, true, None)
                .expect("a decode error after a good round still yields a function");

        assert_ne!(
            diag.stop_reason, "success_after_strict_indirect_retry",
            "the strict re-lift discards the fixed point's entries and edges"
        );
        let dispatch = pcode
            .blocks
            .iter()
            .find(|block| {
                block
                    .ops
                    .last()
                    .is_some_and(|op| op.opcode == fission_pcode::PcodeOpcode::BranchInd)
            })
            .expect("dispatch block");
        let successor_starts = dispatch
            .successors
            .iter()
            .filter_map(|index| pcode.blocks.iter().find(|block| block.index == *index))
            .map(|block| block.start_address)
            .collect::<Vec<_>>();
        assert!(
            successor_starts.contains(&0x1008),
            "the resolved case must stay wired to the dispatch (stop={}, successors={successor_starts:x?})",
            diag.stop_reason
        );
    }
}
