//! Equivalence and scaling verification for canonical indexed VSA fact accumulation (#1248).

use std::time::Instant;

use fission_pcode_ir::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_static::analysis::value_set::{ValueSetAnalyzer, VsaFact};

fn linear_reference_accumulate(stream: &[VsaFact]) -> (Vec<VsaFact>, u64) {
    let mut facts = Vec::new();
    let mut comparisons = 0u64;
    for fact in stream {
        let mut found = false;
        for existing in &facts {
            comparisons += 1;
            if existing == fact {
                found = true;
                break;
            }
        }
        if !found {
            facts.push(fact.clone());
        }
    }
    (facts, comparisons)
}

#[test]
fn indexed_accumulation_matches_linear_reference_on_facts_and_xrefs() {
    let stream = vec![
        VsaFact::DataRead {
            instruction_addr: 0x1000,
            target_addr: 0x2000,
            pcode_op: PcodeOpcode::Load,
        },
        // Exact duplicate suppressed
        VsaFact::DataRead {
            instruction_addr: 0x1000,
            target_addr: 0x2000,
            pcode_op: PcodeOpcode::Load,
        },
        // Same addresses, different opcode (DataWrite) -> distinct fact
        VsaFact::DataWrite {
            instruction_addr: 0x1000,
            target_addr: 0x2000,
            pcode_op: PcodeOpcode::Store,
        },
        // JumpTableTarget sequence [0x3000, 0x3010]
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1010,
            targets: vec![0x3000, 0x3010],
            pcode_op: PcodeOpcode::BranchInd,
        },
        // Exact duplicate sequence suppressed
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1010,
            targets: vec![0x3000, 0x3010],
            pcode_op: PcodeOpcode::BranchInd,
        },
        // Reordered targets remain distinct under VsaFact::Eq while into_xrefs still deduplicates (from, to, kind)
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1010,
            targets: vec![0x3010, 0x3000],
            pcode_op: PcodeOpcode::BranchInd,
        },
        // Different indirect opcode at same site -> distinct fact & Call xref kind
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1010,
            targets: vec![0x3000],
            pcode_op: PcodeOpcode::CallInd,
        },
        // Growing target set across revisit
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1020,
            targets: vec![0x4000],
            pcode_op: PcodeOpcode::BranchInd,
        },
        VsaFact::JumpTableTarget {
            instruction_addr: 0x1020,
            targets: vec![0x4000, 0x4010],
            pcode_op: PcodeOpcode::BranchInd,
        },
    ];

    let (expected_facts, _) = linear_reference_accumulate(&stream);

    let mut analyzer = ValueSetAnalyzer::new();
    for fact in &stream {
        analyzer.push_fact(fact.clone());
    }

    assert_eq!(analyzer.facts(), expected_facts.as_slice());
    assert_eq!(analyzer.fact_count(), expected_facts.len());
    let stats = analyzer.fact_stats();
    assert_eq!(stats.total_emissions, stream.len() as u64);
    assert_eq!(stats.unique_facts, expected_facts.len());
    assert_eq!(stats.duplicate_emissions, 2);

    // Xref semantics preserved identically: into_xrefs produces identical sorted deduplicated xrefs.
    let actual_xrefs: Vec<(u64, u64, u8)> = analyzer
        .into_xrefs()
        .into_iter()
        .map(|x| (x.from_addr, x.to_addr, x.xref_type as u8))
        .collect();
    assert_eq!(
        actual_xrefs,
        vec![
            (
                0x1000,
                0x2000,
                fission_static::analysis::xrefs::XrefType::DataRead as u8
            ),
            (
                0x1000,
                0x2000,
                fission_static::analysis::xrefs::XrefType::DataWrite as u8
            ),
            (
                0x1010,
                0x3000,
                fission_static::analysis::xrefs::XrefType::Jump as u8
            ),
            (
                0x1010,
                0x3010,
                fission_static::analysis::xrefs::XrefType::Jump as u8
            ),
            (
                0x1010,
                0x3000,
                fission_static::analysis::xrefs::XrefType::Call as u8
            ),
            (
                0x1020,
                0x4000,
                fission_static::analysis::xrefs::XrefType::Jump as u8
            ),
            (
                0x1020,
                0x4010,
                fission_static::analysis::xrefs::XrefType::Jump as u8
            ),
        ]
    );

    // clear_facts resets both facts and hash index cleanly so re-insertion works.
    analyzer.clear_facts();
    assert_eq!(analyzer.fact_count(), 0);
    assert!(analyzer.push_fact(stream[0].clone()));
    assert!(!analyzer.push_fact(stream[0].clone()));
    assert_eq!(analyzer.fact_count(), 1);
}

#[test]
fn repeated_cfg_visits_deduplicate_without_quadratic_checks() {
    // 2-block loop that accumulates abstract states over constant steps; block 1 emits Load + Store
    // and revisits multiple times before widening/converging.
    let ptr = Varnode {
        space_id: 1,
        offset: 0x10,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = Varnode {
        space_id: 1,
        offset: 0x20,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let op = |seq, opcode, address, output, inputs| PcodeOp {
        seq_num: seq,
        opcode,
        address,
        output,
        inputs,
        asm_mnemonic: None,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![op(
                    0,
                    PcodeOpcode::Copy,
                    0x1000,
                    Some(ptr.clone()),
                    vec![Varnode::constant(0x5000, 8)],
                )],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![1],
                ops: vec![
                    op(
                        1,
                        PcodeOpcode::Load,
                        0x1010,
                        Some(tmp),
                        vec![Varnode::constant(0, 4), ptr.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::Store,
                        0x1014,
                        None,
                        vec![Varnode::constant(0, 4), ptr.clone(), ptr.clone()],
                    ),
                    op(
                        3,
                        PcodeOpcode::IntAdd,
                        0x1018,
                        Some(ptr.clone()),
                        vec![ptr, Varnode::constant(8, 8)],
                    ),
                ],
            },
        ],
    };

    let mut analyzer = ValueSetAnalyzer::new();
    assert!(analyzer.analyze(&func));
    let stats = analyzer.fact_stats();
    assert!(stats.unique_facts > 0);
    assert!(stats.duplicate_emissions > 0);
    assert!(
        stats.equality_checks <= stats.total_emissions + 16,
        "expected linear equality checks under repeated CFG visits, got {stats:?}"
    );
}

#[test]
fn scaling_1k_10k_100k_unique_and_duplicate_facts_is_amortized_linear() {
    for n in [1_000usize, 10_000usize, 100_000usize] {
        let mut unique_stream = Vec::with_capacity(n);
        for i in 0..n {
            let fact = match i % 3 {
                0 => VsaFact::DataRead {
                    instruction_addr: 0x400000 + (i as u64) * 4,
                    target_addr: 0x600000 + (i as u64) * 8,
                    pcode_op: PcodeOpcode::Load,
                },
                1 => VsaFact::DataWrite {
                    instruction_addr: 0x400000 + (i as u64) * 4,
                    target_addr: 0x700000 + (i as u64) * 8,
                    pcode_op: PcodeOpcode::Store,
                },
                _ => VsaFact::JumpTableTarget {
                    instruction_addr: 0x400000 + (i as u64) * 4,
                    targets: vec![0x800000 + (i as u64) * 4, 0x800004 + (i as u64) * 4],
                    pcode_op: PcodeOpcode::BranchInd,
                },
            };
            unique_stream.push(fact);
        }

        let t0 = Instant::now();
        let mut analyzer = ValueSetAnalyzer::new();
        for f in &unique_stream {
            assert!(analyzer.push_fact(f.clone()));
        }
        let unique_us = t0.elapsed().as_micros();
        let stats_after_unique = analyzer.fact_stats();

        let t1 = Instant::now();
        for f in &unique_stream {
            assert!(!analyzer.push_fact(f.clone()));
        }
        let dup_us = t1.elapsed().as_micros();
        let stats_after_dup = analyzer.fact_stats();

        let linear_summary = if n <= 10_000 {
            let t_lin = Instant::now();
            let (lin_facts, lin_cmp) = linear_reference_accumulate(&unique_stream);
            assert_eq!(lin_facts.len(), n);
            format!(
                "linear_ref_cmp={lin_cmp} linear_ref_us={}",
                t_lin.elapsed().as_micros()
            )
        } else {
            "linear_ref=skipped(>10k O(N^2))".to_string()
        };

        eprintln!(
            "VSA scaling N={n}: unique_us={unique_us} unique_eq_checks={} dup_us={dup_us} total_eq_checks={} dup_suppressed={} | {linear_summary}",
            stats_after_unique.equality_checks,
            stats_after_dup.equality_checks,
            stats_after_dup.duplicate_emissions
        );

        assert_eq!(analyzer.fact_count(), n);
        assert_eq!(analyzer.facts(), unique_stream.as_slice());
        assert!(
            stats_after_unique.equality_checks <= (n as u64) / 50 + 32,
            "unexpectedly high hash collisions at N={n}: {:?}",
            stats_after_unique
        );
        assert_eq!(stats_after_dup.duplicate_emissions, n as u64);
        assert!(
            stats_after_dup.equality_checks <= (n as u64) + (n as u64) / 25 + 64,
            "duplicate replay equality checks should be ~1 per duplicate at N={n}: {:?}",
            stats_after_dup
        );
    }
}
