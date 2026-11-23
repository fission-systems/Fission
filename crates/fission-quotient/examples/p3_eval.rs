//! WBCQ P3 evaluation (RFC #1093) for one step function: the white-box
//! quotient, an independent replay of long random words against it, and the
//! black-box ablation (no P-code summaries, no solver). Prints one JSON line.
//!
//! Usage:
//!   cargo run --release -p fission-quotient --example p3_eval -- \
//!     <binary> <function-name | 0xADDRESS> <context-bytes> [--input-arg I] [--input-bits B]
//!     [--output-bits B] [--replay-words N] [--replay-length L]
//!
//! Driven over the corpus by `scripts/quality/wbcq/evaluate.py`.

use std::path::PathBuf;
use std::time::Instant;

use fission_loader::loader::LoadedBinary;
use fission_quotient::learn::{Learned, Learner};
use fission_quotient::{
    ContextRegion, EmulatorOracle, InputPort, MembershipOracle, QuotientBoundary, QuotientBudget,
    ReplayVerdict, Symbol, Wbcq,
};
use serde_json::json;

/// xorshift64: the same words on every host and every run.
struct Words(u64);

impl Words {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value drawn the way a tester would: often a byte, sometimes any
    /// 32-bit value, sometimes a small number.
    fn value(&mut self) -> u64 {
        match self.next() % 4 {
            0 | 1 => self.next() & 0xff,
            2 => self.next() & 0xf,
            _ => self.next() & 0xffff_ffff,
        }
    }
}

fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn address_of(path: &PathBuf, name: &str) -> anyhow::Result<u64> {
    let binary = LoadedBinary::from_file(path)?;
    let inner = binary.inner();
    let index = *inner
        .function_name_index
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("{name} is not in the symbol table"))?;
    Ok(inner.functions[index].address)
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("binary"));
    let name = args.next().expect("function name");
    let context_bytes: usize = args.next().expect("context bytes").parse()?;
    let (mut replay_words, mut replay_length) = (200usize, 24usize);
    let (mut input_arg, mut input_bits, mut output_bits) = (1usize, 32u32, 32u32);
    while let Some(flag) = args.next() {
        let value: usize = args.next().expect("flag value").parse()?;
        match flag.as_str() {
            "--replay-words" => replay_words = value,
            "--replay-length" => replay_length = value,
            "--input-arg" => input_arg = value,
            "--input-bits" => input_bits = value as u32,
            "--output-bits" => output_bits = value as u32,
            other => anyhow::bail!("unknown flag {other}"),
        }
    }
    let entry = match name.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16)?,
        None => address_of(&path, &name)?,
    };
    let boundary = QuotientBoundary {
        entry,
        inputs: vec![InputPort {
            name: "in".to_string(),
            arg_index: input_arg,
            bits: input_bits,
        }],
        context: ContextRegion {
            arg_index: 0,
            initial: vec![0; context_bytes],
        },
        output_bits,
    };
    let budget = QuotientBudget::default();

    // White box.
    let started = Instant::now();
    let (mut engine, foundation) = Wbcq::for_binary(&path, boundary.clone(), 1_000_000)?;
    let (report, model) = engine.synthesize_with_model(&foundation, budget)?;
    let white_ms = started.elapsed().as_millis();

    // Independent replay: long words the learner never chose.
    let mut words = Words(0x51_7e_c0de);
    let alphabet = report.evidence.alphabet.clone();
    let mut replay = json!(null);
    if let Some(model) = &model {
        let mut oracle = EmulatorOracle::new(&path, &boundary, 1_000_000)?;
        let (mut agree, mut result_differs, mut data_differs) = (0u64, 0u64, 0u64);
        let mut first_difference = None;
        for _ in 0..replay_words {
            let word: Vec<Symbol> = (0..replay_length)
                .map(|_| {
                    if !alphabet.is_empty() && words.next() % 2 == 0 {
                        alphabet[(words.next() % alphabet.len() as u64) as usize].clone()
                    } else {
                        vec![words.value() & mask(input_bits)]
                    }
                })
                .collect();
            let records = oracle.run(&word)?;
            match model.compare(&word, &records) {
                ReplayVerdict::Agrees => agree += 1,
                ReplayVerdict::ResultDiffers => {
                    result_differs += 1;
                    first_difference.get_or_insert_with(|| word.clone());
                }
                ReplayVerdict::DataDiffers => {
                    data_differs += 1;
                    first_difference.get_or_insert_with(|| word.clone());
                }
            }
        }
        // W-method conformance: every state's access word, each alphabet
        // symbol, then each distinguishing suffix (and none). Random words
        // rarely reach a state behind a keyword; these reach every one.
        let mut suffixes: Vec<Vec<Symbol>> = vec![Vec::new()];
        for witness in &report.apartness_witnesses {
            if !suffixes.contains(&witness.input_sequence) {
                suffixes.push(witness.input_sequence.clone());
            }
        }
        let (mut conformance_words, mut conformance_agree) = (0u64, 0u64);
        let mut conformance_difference = None;
        for state in &report.states {
            for symbol in &alphabet {
                for suffix in &suffixes {
                    let mut word = state.access_word.clone();
                    word.push(symbol.clone());
                    word.extend(suffix.iter().cloned());
                    let records = oracle.run(&word)?;
                    conformance_words += 1;
                    if model.compare(&word, &records) == ReplayVerdict::Agrees {
                        conformance_agree += 1;
                    } else {
                        conformance_difference.get_or_insert_with(|| word.clone());
                    }
                }
            }
        }
        replay = json!({
            "words": replay_words,
            "length": replay_length,
            "agree": agree,
            "result_differs": result_differs,
            "data_differs": data_differs,
            "first_difference": first_difference,
            "conformance_words": conformance_words,
            "conformance_agree": conformance_agree,
            "conformance_difference": conformance_difference,
        });
    }

    // Black box: an alphabet a tester would sample, random-walk equivalence,
    // the same learner and the same membership oracle.
    let started = Instant::now();
    let mut sampler = Words(0xb1ac_b0c5);
    let mut black_alphabet: Vec<Symbol> = vec![vec![0], vec![1]];
    while black_alphabet.len() < 16 {
        let symbol = vec![sampler.value() & mask(input_bits)];
        if !black_alphabet.contains(&symbol) {
            black_alphabet.push(symbol);
        }
    }
    let mut oracle = EmulatorOracle::new(&path, &boundary, 1_000_000)?;
    let mut learner = Learner::new(black_alphabet, boundary.context.initial.clone());
    let mut black = json!(null);
    // A query the oracle refuses (a nondeterministic step) ends the black
    // box with that reason; it is a result of the ablation, not a failure.
    let mut learn_black = |learner: &mut Learner,
                           oracle: &mut EmulatorOracle|
     -> anyhow::Result<serde_json::Value> {
        for _round in 0..64 {
            match learner.learn(oracle, budget.max_states)? {
                Learned::Partial { states, .. } => {
                    return Ok(json!({"status": "partial", "states": states}));
                }
                Learned::Hypothesis(hypothesis) => {
                    let mut counterexample = None;
                    for _ in 0..replay_words {
                        let word = (0..replay_length)
                            .map(|_| (sampler.next() % learner.alphabet.len() as u64) as usize)
                            .collect::<Vec<_>>();
                        let observed = learner.query(oracle, &word)?;
                        if observed != hypothesis.run_from(0, &word).0 {
                            counterexample = Some(word);
                            break;
                        }
                    }
                    match counterexample {
                        Some(word) => learner.process_counterexample(oracle, &hypothesis, &word)?,
                        None => {
                            return Ok(json!({
                                "status": "consistent",
                                "states": hypothesis.states.len(),
                            }));
                        }
                    }
                }
            }
        }
        Ok(json!({"status": "rounds", "states": learner.basis().len()}))
    };
    match learn_black(&mut learner, &mut oracle) {
        Ok(value) => black = value,
        Err(error) => {
            black = json!({"status": "error", "reason": error.to_string(), "states": null});
        }
    }
    black["membership_queries"] = json!(learner.queries());
    black["emulator_steps"] = json!(oracle.steps());
    black["elapsed_ms"] = json!(started.elapsed().as_millis());

    let line = json!({
        "binary": path,
        "function": name,
        "white_box": {
            "status": report.status,
            "states": report.metrics.quotient_states,
            "transitions": report.metrics.quotient_transitions,
            "metrics": report.metrics,
            "evidence": {
                "membership_queries": report.evidence.membership_queries,
                "emulator_steps": report.evidence.emulator_steps,
                "refutation_queries": report.evidence.refutation_queries,
                "solver_unknowns": report.evidence.solver_unknowns,
                "rounds": report.evidence.rounds,
                "alphabet": report.evidence.alphabet.len(),
            },
            "slots": report.slots,
            "witnesses_verified": report.apartness_witnesses.iter().all(|w| w.verified_by_emulator),
            "witnesses": report.apartness_witnesses.len(),
            "elapsed_ms": white_ms,
        },
        "replay": replay,
        "black_box": black,
    });
    println!("{line}");
    Ok(())
}
