use fission_loader::LoadedBinary;
use fission_sleigh::runtime::{PackedContextOverride, RuntimeSleighFrontend};

use super::coverage_decode::{COVERAGE_WINDOW_BYTES, CoverageDecodes, CoverageInstruction};
use super::discovery_diag_enabled;
use super::targets::collect_instruction_targets;
use super::thumb::image_decode_context;

pub(super) struct ControlFlowReferences {
    pub(super) decoded_instruction_count: usize,
    pub(super) call_targets: Vec<u64>,
    pub(super) jump_targets: Vec<u64>,
    pub(super) jump_edges: Vec<(u64, u64)>,
    pub(super) coverage_decodes: CoverageDecodes,
}

pub(super) fn scan_executable_sections(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
) -> ControlFlowReferences {
    // A stripped Cortex-M image is all Thumb and says so nowhere in its
    // addresses; without this the sweep below decodes ARM-mode nonsense over
    // it and finds no targets at all.
    let sweep_context = image_decode_context(binary, frontend);
    if discovery_diag_enabled() && sweep_context.is_some() {
        eprintln!("DISCOVERY_DIAG: sweeping in thumb mode");
    }

    struct ScanChunk<'a> {
        bytes: &'a [u8],
        virtual_address: u64,
    }

    // Collect each executable section's usable byte range first so the chunk
    // size below can be sized against the *total* executable footprint, not
    // decided per-section in isolation.
    let mut section_ranges: Vec<(&[u8], u64)> = Vec::new();
    let mut total_executable_bytes = 0usize;
    for section in binary
        .sections
        .iter()
        .filter(|section| section.is_executable)
    {
        let file_start = section.file_offset as usize;
        let size = section.file_size.min(section.virtual_size) as usize;
        if size == 0 || file_start >= binary.data.as_slice().len() {
            continue;
        }
        let file_end = file_start
            .saturating_add(size)
            .min(binary.data.as_slice().len());
        if file_end <= file_start {
            continue;
        };
        let bytes = &binary.data.as_slice()[file_start..file_end];
        total_executable_bytes += bytes.len();
        section_ranges.push((bytes, section.virtual_address));
    }

    // A fixed 512KB chunk size starves this scan's own parallelism on any
    // binary whose executable sections total a few MB or less -- a 617KB
    // `.text` (a fairly ordinary Rust binary) yields only 2 chunks no matter
    // how many cores are available. Instead, target roughly
    // `4x` the current thread count worth of chunks (the 4x over-partitions
    // so an uneven chunk -- one with more decodable code, more call/jump
    // targets -- doesn't leave other threads idle near the end), floored at
    // a minimum so tiny binaries don't pay per-chunk overhead for no
    // benefit.
    const MIN_DISCOVERY_CHUNK_SIZE: usize = 64 * 1024; // 64KB
    let target_chunk_count = rayon::current_num_threads().max(1) * 4;
    let discovery_chunk_size =
        (total_executable_bytes / target_chunk_count.max(1)).max(MIN_DISCOVERY_CHUNK_SIZE);

    let mut chunks = Vec::new();
    for (bytes, virtual_address) in section_ranges {
        let mut offset = 0;
        while offset < bytes.len() {
            let chunk_end = (offset + discovery_chunk_size).min(bytes.len());
            let chunk_bytes = &bytes[offset..chunk_end];
            let chunk_va = virtual_address + offset as u64;
            chunks.push(ScanChunk {
                bytes: chunk_bytes,
                virtual_address: chunk_va,
            });
            offset = chunk_end;
        }
    }
    if discovery_diag_enabled() {
        eprintln!(
            "DISCOVERY_DIAG: chunks={} total_bytes={} chunk_size={}",
            chunks.len(),
            chunks.iter().map(|c| c.bytes.len()).sum::<usize>(),
            discovery_chunk_size,
        );
    }

    use rayon::prelude::*;
    let stage_start = std::time::Instant::now();

    // Bound the transaction's extra memory and spread it across sections.
    const MAX_COVERAGE_DECODES: usize = 256 * 1024;
    let (total_decoded, call_targets, jump_targets, jump_edges, coverage_rows) = chunks
        .into_par_iter()
        .map(|chunk| {
            let mut local_calls = Vec::with_capacity(4096);
            let mut local_jumps = Vec::with_capacity(4096);
            let mut local_edges = Vec::with_capacity(1024);
            let mut coverage_rows = Vec::new();
            let coverage_budget =
                chunk.bytes.len().saturating_mul(MAX_COVERAGE_DECODES) / total_executable_bytes;
            let count = scan_section_references(
                binary,
                frontend,
                chunk.bytes,
                chunk.virtual_address,
                sweep_context,
                &mut local_calls,
                &mut local_jumps,
                &mut local_edges,
                &mut coverage_rows,
                coverage_budget,
            );
            local_calls.sort_unstable();
            local_calls.dedup();
            local_jumps.sort_unstable();
            local_jumps.dedup();
            local_edges.sort_unstable();
            local_edges.dedup();
            (count, local_calls, local_jumps, local_edges, coverage_rows)
        })
        .reduce(
            || (0usize, Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |(count_a, mut calls_a, mut jumps_a, mut edges_a, mut rows_a),
             (count_b, mut calls_b, mut jumps_b, mut edges_b, mut rows_b)| {
                calls_a.append(&mut calls_b);
                calls_a.sort_unstable();
                calls_a.dedup();

                jumps_a.append(&mut jumps_b);
                jumps_a.sort_unstable();
                jumps_a.dedup();

                edges_a.append(&mut edges_b);
                edges_a.sort_unstable();
                edges_a.dedup();

                rows_a.append(&mut rows_b);
                (count_a + count_b, calls_a, jumps_a, edges_a, rows_a)
            },
        );

    let coverage_decodes: CoverageDecodes = coverage_rows.into_iter().collect();
    if discovery_diag_enabled() {
        eprintln!(
            "DISCOVERY_DIAG: scan_stage={:?} decoded={} calls={} jumps={}",
            stage_start.elapsed(),
            total_decoded,
            call_targets.len(),
            jump_targets.len()
        );
        eprintln!(
            "DISCOVERY_DIAG: coverage_records={}",
            coverage_decodes.len()
        );
    }

    ControlFlowReferences {
        decoded_instruction_count: total_decoded,
        call_targets,
        jump_targets,
        jump_edges,
        coverage_decodes,
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_section_references(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    bytes: &[u8],
    base_address: u64,
    sweep_context: Option<PackedContextOverride>,
    call_targets: &mut Vec<u64>,
    jump_targets: &mut Vec<u64>,
    jump_edges: &mut Vec<(u64, u64)>,
    coverage_rows: &mut Vec<((u64, usize), (usize, CoverageInstruction))>,
    coverage_budget: usize,
) -> usize {
    let mut decoded_count = 0usize;
    let mut offset = 0usize;
    let mut current = base_address;

    while offset < bytes.len() {
        let remaining = &bytes[offset..];
        let collect_coverage = sweep_context.is_none() && coverage_rows.len() < coverage_budget;
        if let Ok(instructions) = frontend.decode_window_no_pcode_with_read_prefixes(
            remaining,
            current,
            remaining.len().min(4096),
            sweep_context,
            collect_coverage,
        ) {
            if !instructions.is_empty() {
                let mut batch_bytes = 0;
                for (instruction, prefix) in &instructions {
                    if let Some(prefix) = *prefix
                        && prefix <= COVERAGE_WINDOW_BYTES
                        && coverage_rows.len() < coverage_budget
                        && let Some(coverage) = CoverageInstruction::from_decoded(instruction)
                    {
                        let input = remaining.as_ptr() as usize + batch_bytes;
                        coverage_rows.push(((instruction.address, input), (prefix, coverage)));
                    }
                    batch_bytes += instruction.length;
                    collect_instruction_targets(
                        binary,
                        instruction,
                        call_targets,
                        jump_targets,
                        jump_edges,
                    );
                    decoded_count += 1;
                }
                offset = offset.saturating_add(batch_bytes);
                current = current.saturating_add(batch_bytes as u64);
                continue;
            }
        }

        match frontend.decode_single_no_pcode(remaining, current, sweep_context) {
            Ok(instruction) if instruction.length > 0 && instruction.length <= remaining.len() => {
                let step = instruction.length;
                collect_instruction_targets(
                    binary,
                    &instruction,
                    call_targets,
                    jump_targets,
                    jump_edges,
                );
                decoded_count += 1;
                offset = offset.saturating_add(step);
                current = current.saturating_add(step as u64);
            }
            _ => {
                // Thumb instructions are 2-byte aligned, so resyncing a byte
                // at a time only produces misaligned decodes of the same
                // bytes; step by the architecture's own alignment instead.
                let step = if sweep_context.is_some() { 2 } else { 1 };
                offset = offset.saturating_add(step);
                current = current.saturating_add(step as u64);
            }
        }
    }

    decoded_count
}
