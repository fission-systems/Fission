//! Fission Performance Benchmarks
//!
//! Benchmarks for key components:
//! - Binary parsing (PE, ELF)
//! - Disassembly
//! - Function signature matching
//! - Cross-reference analysis

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

// Import Fission components for benchmarking
use fission::analysis::{
    disasm::DisasmEngine,
    signatures::SignatureDatabase,
    xrefs::{Xref, XrefDatabase, XrefType},
};

/// Benchmark XrefDatabase operations
fn bench_xref_database(c: &mut Criterion) {
    let mut group = c.benchmark_group("xref_database");

    // Benchmark adding cross-references
    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("add_xref", size), size, |b, &size| {
            b.iter(|| {
                let mut db = XrefDatabase::new();
                for i in 0..size {
                    db.add_xref(Xref {
                        from_addr: 0x1000 + i as u64 * 4,
                        to_addr: 0x2000 + (i % 100) as u64 * 8,
                        xref_type: XrefType::Call,
                    });
                }
                black_box(db)
            });
        });
    }

    // Benchmark total_refs (should be O(1) now)
    group.bench_function("total_refs_1000", |b| {
        let mut db = XrefDatabase::new();
        for i in 0..1000 {
            db.add_xref(Xref {
                from_addr: 0x1000 + i as u64 * 4,
                to_addr: 0x2000 + (i % 100) as u64 * 8,
                xref_type: XrefType::Call,
            });
        }
        b.iter(|| black_box(db.total_refs()));
    });

    group.finish();
}

/// Benchmark signature database operations
fn bench_signature_database(c: &mut Criterion) {
    let mut group = c.benchmark_group("signature_database");

    // Create database once
    let db = SignatureDatabase::new();

    // Test pattern that should match
    let matching_bytes = vec![
        0x48, 0x83, 0xEC, 0x28, // sub rsp, 0x28
        0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, // mov rax, [rip+X]
    ];

    // Test pattern that won't match
    let non_matching_bytes = vec![0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90];

    group.bench_function("identify_matching", |b| {
        b.iter(|| black_box(db.identify(&matching_bytes)));
    });

    group.bench_function("identify_non_matching", |b| {
        b.iter(|| black_box(db.identify(&non_matching_bytes)));
    });

    group.finish();
}

/// Benchmark disassembly operations
fn bench_disassembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("disassembly");

    // Sample x64 code (function prologue + some ops)
    let code_64 = vec![
        0x55, // push rbp
        0x48, 0x89, 0xE5, // mov rbp, rsp
        0x48, 0x83, 0xEC, 0x20, // sub rsp, 0x20
        0x48, 0x89, 0x7D, 0xF8, // mov [rbp-8], rdi
        0x48, 0x8B, 0x45, 0xF8, // mov rax, [rbp-8]
        0x48, 0x83, 0xC4, 0x20, // add rsp, 0x20
        0x5D, // pop rbp
        0xC3, // ret
    ];

    // Sample x86 code
    let code_32 = vec![
        0x55, // push ebp
        0x89, 0xE5, // mov ebp, esp
        0x83, 0xEC, 0x10, // sub esp, 0x10
        0x89, 0x45, 0xFC, // mov [ebp-4], eax
        0x8B, 0x45, 0xFC, // mov eax, [ebp-4]
        0xC9, // leave
        0xC3, // ret
    ];

    let engine_64 = DisasmEngine::new(true).unwrap();
    let engine_32 = DisasmEngine::new(false).unwrap();

    group.bench_function("disasm_x64_small", |b| {
        b.iter(|| black_box(engine_64.disassemble(&code_64, 0x1000)));
    });

    group.bench_function("disasm_x86_small", |b| {
        b.iter(|| black_box(engine_32.disassemble(&code_32, 0x1000)));
    });

    // Larger code block (repeated)
    let large_code: Vec<u8> = code_64.iter().cycle().take(1024).copied().collect();

    group.bench_function("disasm_x64_1kb", |b| {
        b.iter(|| black_box(engine_64.disassemble(&large_code, 0x1000)));
    });

    group.finish();
}

/// Benchmark call target discovery
fn bench_call_discovery(c: &mut Criterion) {
    let mut group = c.benchmark_group("call_discovery");

    // Code with multiple call instructions
    let code_with_calls: Vec<u8> = {
        let mut code = Vec::new();
        for i in 0..100 {
            // call rel32 (E8 XX XX XX XX)
            code.push(0xE8);
            let offset = (i * 0x100) as i32;
            code.extend_from_slice(&offset.to_le_bytes());
            // Some NOPs between calls
            code.extend_from_slice(&[0x90, 0x90, 0x90]);
        }
        code
    };

    let engine = DisasmEngine::new(true).unwrap();

    group.bench_function("discover_100_calls", |b| {
        b.iter(|| black_box(engine.discover_call_targets(&code_with_calls, 0x1000)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_xref_database,
    bench_signature_database,
    bench_disassembly,
    bench_call_discovery,
);
criterion_main!(benches);
