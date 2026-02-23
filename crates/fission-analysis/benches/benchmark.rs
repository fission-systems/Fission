use criterion::{Criterion, black_box, criterion_group, criterion_main};

// This is a placeholder for CFG analysis benchmarking.
// In the future, you can import CfgAnalysis and measure its performance on real binary data.
fn cfg_analysis_benchmark(c: &mut Criterion) {
    c.bench_function("cfg_analysis_dummy", |b| {
        b.iter(|| {
            // FIXME: Replace with a real fixture once the bench harness is wired up.
            //
            // Suggested pattern:
            //   let elf_bytes = include_bytes!(concat!(
            //       env!("CARGO_MANIFEST_DIR"), "/../../tests/binaries/comparison_test_x64"
            //   ));
            //   let binary = fission_loader::load_bytes(elf_bytes).unwrap();
            //   let arc  = Arc::new(binary);
            //   let cfg  = CfgAnalysis::new(arc);
            //   black_box(cfg.run());
            black_box(1 + 1);
        })
    });
}

criterion_group!(benches, cfg_analysis_benchmark);
criterion_main!(benches);
