use criterion::{Criterion, black_box, criterion_group, criterion_main};
use fission_analysis::analysis::cfg::CfgAnalysis;
use fission_analysis::analysis::optimizer::OptimizerConfig;
use fission_analysis::analysis::optimizer::integration::optimize_c_code;
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

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
            ops,
        });
    }
    PcodeFunction { blocks }
}

fn cfg_analysis_benchmark(c: &mut Criterion) {
    let func = build_diamond_cfg(64);
    c.bench_function("cfg_analysis_64_blocks", |b| {
        b.iter(|| {
            let result = CfgAnalysis::from_pcode(black_box(&func));
            black_box(result)
        })
    });

    let func_large = build_diamond_cfg(256);
    c.bench_function("cfg_analysis_256_blocks", |b| {
        b.iter(|| {
            let result = CfgAnalysis::from_pcode(black_box(&func_large));
            black_box(result)
        })
    });
}

fn optimizer_benchmark(c: &mut Criterion) {
    let sample_code = r#"
    int x = a ^ 0;
    int y = b + 0;
    int z = c * 1;
    if (x > 0) {
        result = x + y;
    }
    return result;
"#;
    c.bench_function("optimizer_simple", |b| {
        b.iter(|| {
            let result = optimize_c_code(black_box(sample_code), OptimizerConfig::default());
            black_box(result)
        })
    });
}

fn binary_load_benchmark(c: &mut Criterion) {
    // Use the PE test binary shipped with the repo (small, self-contained)
    let pe_bytes: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/binaries/comparison_test_x64.exe"
    ));

    c.bench_function("load_pe_binary", |b| {
        b.iter(|| {
            let binary = fission_loader::LoadedBinary::from_bytes(
                black_box(pe_bytes.to_vec()),
                "bench.exe".to_string(),
            );
            black_box(binary)
        })
    });
}

criterion_group!(
    benches,
    cfg_analysis_benchmark,
    optimizer_benchmark,
    binary_load_benchmark
);
criterion_main!(benches);
