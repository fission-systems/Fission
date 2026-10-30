//! `decode_window_chunked` must return exactly what the sequential sweep does,
//! whatever the seams fall on.

use super::*;

/// Deterministic byte stream; no rand dependency.
fn xorshift(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Valid x86-64 of mixed lengths (1-7 bytes) with a few long forms, so a nominal
/// seam usually lands inside an instruction.
fn x86_stream(repeats: usize) -> Vec<u8> {
    let pieces: [&[u8]; 9] = [
        &[0x55],
        &[0x48, 0x89, 0xe5],
        &[0x48, 0x83, 0xec, 0x10],
        &[0x89, 0x7d, 0xfc],
        &[0xe8, 0x10, 0x00, 0x00, 0x00],
        &[0x0f, 0x1f, 0x44, 0x00, 0x00],
        &[0x48, 0x8d, 0x05, 0x34, 0x12, 0x00, 0x00],
        &[0xb8, 0x01, 0x00, 0x00, 0x00],
        &[0xc3],
    ];
    let mut out = Vec::new();
    for i in 0..repeats {
        out.extend_from_slice(pieces[i % pieces.len()]);
        out.extend_from_slice(pieces[(i * 7 + 3) % pieces.len()]);
    }
    out
}

fn assert_same(
    frontend: &RuntimeSleighFrontend,
    bytes: &[u8],
    base: u64,
    chunk_len: usize,
    threads: usize,
) {
    let sequential = frontend.decode_window(bytes, base, usize::MAX);
    let chunked = frontend.decode_window_chunked(bytes, base, chunk_len, threads);
    match (sequential, chunked) {
        (Ok(a), Ok(b)) => assert!(
            a == b,
            "chunk_len {chunk_len} threads {threads}: {} vs {} instructions, first difference at {:?}",
            a.len(),
            b.len(),
            a.iter().zip(&b).position(|(x, y)| x != y)
        ),
        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "chunk_len {chunk_len}"),
        (a, b) => panic!(
            "chunk_len {chunk_len}: sequential ok={} chunked ok={}",
            a.is_ok(),
            b.is_ok()
        ),
    }
}

#[test]
fn chunked_decode_equals_sequential_on_valid_x86_code() {
    let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
    let bytes = x86_stream(6000);
    let decoded = frontend
        .decode_window(&bytes, 0x40_0000, usize::MAX)
        .unwrap();
    assert!(decoded.len() > 10_000, "stream must span many seams");
    for chunk_len in [1000, 1001, 4093, 4096, 7777, 20_000] {
        for threads in [2, 4] {
            assert_same(&frontend, &bytes, 0x40_0000, chunk_len, threads);
        }
    }
}

#[test]
fn chunked_decode_equals_sequential_when_the_sweep_stops_midway() {
    let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
    let mut bytes = x86_stream(4000);
    // 0x06 is not an instruction in 64-bit mode: the sweep ends there, and a
    // speculative piece starting later still decodes happily past it.
    for at in [bytes.len() / 3, bytes.len() / 2] {
        let mut stopped = bytes.clone();
        stopped[at] = 0x06;
        stopped[at + 1] = 0x06;
        for chunk_len in [999, 4096, 10_007] {
            assert_same(&frontend, &stopped, 0x1000, chunk_len, 3);
        }
    }
    // A truncated final instruction, and data that never decodes at all.
    bytes.push(0xe8);
    assert_same(&frontend, &bytes, 0x1000, 4096, 4);
    assert_same(&frontend, &[0x06; 5000], 0x1000, 1024, 4);
}

#[test]
fn chunked_decode_equals_sequential_on_arbitrary_bytes() {
    let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
    for seed in 1..=8u64 {
        let bytes = xorshift(seed, 120_000);
        for chunk_len in [511, 4096, 30_000] {
            assert_same(&frontend, &bytes, 0x7000_0000, chunk_len, 4);
        }
    }
}

/// ARM queues context commits (`globalset`), so seams must not carry state the
/// speculative piece never saw.
#[test]
fn chunked_decode_equals_sequential_where_context_commits_exist() {
    let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").unwrap();
    for seed in 1..=6u64 {
        let bytes = xorshift(seed * 977, 80_000);
        for chunk_len in [1000, 4097, 20_000] {
            assert_same(&frontend, &bytes, 0x1_0000, chunk_len, 4);
        }
    }
    let frontend = RuntimeSleighFrontend::new_for_language("ARM8m_le").unwrap();
    for seed in 1..=6u64 {
        let bytes = xorshift(seed * 31, 80_000);
        for chunk_len in [1000, 4097, 20_000] {
            assert_same(&frontend, &bytes, 0x800_0000, chunk_len, 4);
        }
    }
}

#[test]
fn chunked_decode_degrades_to_the_sequential_sweep() {
    let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
    let bytes = x86_stream(500);
    // One thread, one chunk, empty input, and an address window that would
    // overflow all take the plain path.
    assert_same(&frontend, &bytes, 0x1000, 100, 1);
    assert_same(&frontend, &bytes, 0x1000, bytes.len() + 1, 4);
    assert_same(&frontend, &[], 0x1000, 100, 4);
    assert_same(&frontend, &bytes, u64::MAX - 10, 100, 4);
}

/// Valid code with short random islands, cut into pieces much smaller than the
/// islands' spacing, so speculative pieces start misaligned, fail, resume, and
/// the real stream has to find them again.
#[test]
fn chunked_decode_equals_sequential_with_data_islands_and_tiny_pieces() {
    let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
    for seed in 1..=12u64 {
        let mut bytes = x86_stream(900);
        let noise = xorshift(seed * 7919, 4096);
        // Overwrite a few islands with noise; keep most of the stream valid.
        for island in 0..4usize {
            let at = (island + 1) * bytes.len() / 6;
            let len = 1 + (noise[island] as usize % 24);
            for i in 0..len.min(bytes.len() - at) {
                bytes[at + i] = noise[100 + island * 31 + i];
            }
        }
        for chunk_len in [17, 64, 131, 300, 1024] {
            for threads in [2, 5] {
                assert_same(&frontend, &bytes, 0x40_1000, chunk_len, threads);
            }
        }
    }
}
