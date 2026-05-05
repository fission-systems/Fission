//! Criterion benchmarks for static analysis hot paths.
//!
//! PE/binary loading benchmarks run only when **`FISSION_BENCH_PE_CORPUS`** is set to an existing
//! directory that mirrors the layout under `benchmark/binary/x86-64/window` (`small`/`medium`/`large`
//! plus optional `commercial_binary`). Without it, those benches are skipped so the crate does not
//! assume the repo `benchmark/` tree.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fission_pcode::{
    PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode, cfg::CfgAnalysis,
};
use fission_static::analysis::optimizer::OptimizerConfig;
use fission_static::analysis::optimizer::integration::optimize_c_code;
use std::fs;
use std::path::PathBuf;

/// Build a synthetic PcodeFunction with `n` blocks forming a diamond CFG.
fn build_diamond_cfg(n: usize) -> PcodeFunction {
    let mut blocks = Vec::with_capacity(n);
    for i in 0..n {
        let addr = (0x1000 + i * 0x10) as u64;
        let ops = vec![PcodeOp {
            seq_num: i as u32,
            opcode: if i == n - 1 {
                PcodeOpcode::Return
            } else {
                PcodeOpcode::Branch
            },
            address: addr,
            output: None,
            inputs: vec![Varnode {
                space_id: 0,
                offset: addr + 0x10,
                size: 8,
                is_constant: false,
                constant_val: 0,
            }],
            asm_mnemonic: None,
        }];
        blocks.push(PcodeBasicBlock {
            index: i as u32,
            start_address: addr,
            successors: if i + 1 < n {
                vec![(i + 1) as u32]
            } else {
                vec![]
            },
            ops,
        });
    }
    PcodeFunction { blocks }
}

/// Build a complex CFG with multiple branches and loops
fn build_complex_cfg(depth: usize, branches: usize) -> PcodeFunction {
    let mut blocks = Vec::new();
    let mut block_id = 0u32;

    // Generate nested block structure
    for d in 0..depth {
        for b in 0..branches {
            let addr = (0x2000 + d as u64 * 0x1000 + b as u64 * 0x100) as u64;
            let ops = vec![PcodeOp {
                seq_num: block_id,
                opcode: if d == depth - 1 && b == branches - 1 {
                    PcodeOpcode::Return
                } else {
                    PcodeOpcode::CBranch
                },
                address: addr,
                output: None,
                inputs: vec![Varnode {
                    space_id: 0,
                    offset: addr + 0x10,
                    size: 8,
                    is_constant: false,
                    constant_val: 0,
                }],
                asm_mnemonic: None,
            }];
            let is_last = d == depth - 1 && b == branches - 1;
            blocks.push(PcodeBasicBlock {
                index: block_id,
                start_address: addr,
                successors: if is_last { vec![] } else { vec![block_id + 1] },
                ops,
            });
            block_id += 1;
        }
    }

    PcodeFunction { blocks }
}

fn cfg_analysis_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfg_analysis");

    // Benchmark different CFG sizes
    for size in [16, 64, 256].iter() {
        let func = build_diamond_cfg(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let result = CfgAnalysis::from_pcode(black_box(&func));
                black_box(result)
            })
        });
    }

    // Benchmark complex CFGs
    for (depth, branches) in [(2, 4), (3, 4), (4, 2)].iter() {
        let func = build_complex_cfg(*depth, *branches);
        let name = format!("complex_d{}_b{}", depth, branches);
        group.bench_with_input(BenchmarkId::from_parameter(&name), &name, |b, _| {
            b.iter(|| {
                let result = CfgAnalysis::from_pcode(black_box(&func));
                black_box(result)
            })
        });
    }

    group.finish();
}

fn optimizer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimizer");

    let simple_code = r#"
    int x = a ^ 0;
    int y = b + 0;
    int z = c * 1;
    if (x > 0) {
        result = x + y;
    }
    return result;
"#;

    let complex_code = r#"
    int sum = 0;
    for (int i = 0; i < 1000; i++) {
        int temp = (i * 2) & 0xFF;
        int opt = temp | 0;
        sum += opt;
        if (opt > 0) {
            sum = sum ^ 0;
        }
    }
    int final = sum * 1;
    return final;
"#;

    group.bench_function("simple_optimization", |b| {
        b.iter(|| {
            let result = optimize_c_code(black_box(simple_code), OptimizerConfig::default());
            black_box(result)
        })
    });

    group.bench_function("complex_optimization", |b| {
        b.iter(|| {
            let result = optimize_c_code(black_box(complex_code), OptimizerConfig::default());
            black_box(result)
        })
    });

    group.finish();
}

fn binary_load_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_loading");
    group.sample_size(50); // Reduce sample size for I/O-heavy benchmark

    let binary_dir: PathBuf = match std::env::var_os("FISSION_BENCH_PE_CORPUS") {
        Some(raw) => {
            let p = PathBuf::from(raw);
            if p.is_dir() {
                p
            } else {
                group.finish();
                return;
            }
        }
        None => {
            group.finish();
            return;
        }
    };

    // Benchmark different binary sizes
    let sizes = vec!["small", "medium", "large"];

    for size in sizes {
        let size_dir = binary_dir.join(size);

        if size_dir.exists() {
            if let Ok(entries) = fs::read_dir(&size_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && (path
                            .extension()
                            .map_or(false, |ext| ext == "exe" || ext == "dll" || ext == "sys"))
                    {
                        let binary_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");

                        if let Ok(bytes) = fs::read(&path) {
                            let benchmark_id = format!("{}_{}", size, binary_name);
                            group.bench_with_input(
                                BenchmarkId::from_parameter(&benchmark_id),
                                &benchmark_id,
                                |b, _| {
                                    let binary_bytes = bytes.clone();
                                    b.iter(|| {
                                        let binary = fission_loader::LoadedBinary::from_bytes(
                                            black_box(binary_bytes.clone()),
                                            binary_name.to_string(),
                                        );
                                        black_box(binary)
                                    })
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    // Also benchmark commercial binaries if available
    let commercial_dir = binary_dir.join("commercial_binary");
    if commercial_dir.exists() {
        if let Ok(entries) = fs::read_dir(&commercial_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && (path
                        .extension()
                        .map_or(false, |ext| ext == "exe" || ext == "dll" || ext == "sys"))
                {
                    let binary_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    if let Ok(bytes) = fs::read(&path) {
                        let benchmark_id = format!("commercial_{}", binary_name);
                        group.bench_with_input(
                            BenchmarkId::from_parameter(&benchmark_id),
                            &benchmark_id,
                            |b, _| {
                                let binary_bytes = bytes.clone();
                                b.iter(|| {
                                    let binary = fission_loader::LoadedBinary::from_bytes(
                                        black_box(binary_bytes.clone()),
                                        binary_name.to_string(),
                                    );
                                    black_box(binary)
                                })
                            },
                        );
                    }
                }
            }
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    cfg_analysis_benchmark,
    optimizer_benchmark,
    binary_load_benchmark
);
criterion_main!(benches);
