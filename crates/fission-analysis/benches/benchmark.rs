use criterion::{Criterion, black_box, criterion_group, criterion_main};

// This is a placeholder for CFG analysis benchmarking.
// In the future, you can import CfgAnalysis and measure its performance on real binary data.
fn cfg_analysis_benchmark(c: &mut Criterion) {
    c.bench_function("cfg_analysis_dummy", |b| {
        b.iter(|| {
            // TODO: Load a LoadedBinary mock and perform CfgAnalysis
            black_box(1 + 1);
        })
    });
}

criterion_group!(benches, cfg_analysis_benchmark);
criterion_main!(benches);
