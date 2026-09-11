//! How much of the benchmark corpus this emulator can *translate*.
//!
//! Ignored by default; it walks hundreds of binaries. Run it with
//!
//! ```text
//! cargo test --release -p fission-emulator --test translation_coverage \
//!     -- --ignored --nocapture
//! ```
//!
//! # Nothing here executes a corpus binary, and nothing here may
//!
//! The DecBench corpus contains malware compiled from source. The rule for it
//! is static analysis only: never run, never `chmod +x`, never load into an
//! emulator "just to see". This benchmark obeys that by construction -- it
//! reads bytes, decodes them, lifts them to p-code and hands the p-code to the
//! JIT. Compiling is not executing: the machine code the compiler returns is
//! never called, and there is no `Emulator` here to call it with.
//!
//! That restriction is what makes this corpus usable at all. Execution
//! coverage has to stay on `corpus/dev`, which we compiled ourselves and which
//! is a hundred binaries; translation coverage gets all 803, with every
//! architecture and optimisation level in the benchmark, at zero risk.
//!
//! # What it measures
//!
//! Three gaps, each of which makes the emulator quietly wrong rather than
//! loudly broken:
//!
//! 1. **Decode failures** -- SLEIGH could not turn the bytes into an
//!    instruction.
//! 2. **Opcodes lowered to nothing** -- the JIT met a p-code op it does not
//!    implement and emitted no code for it. The block still compiles and still
//!    runs; it just computes the wrong thing.
//! 3. **Userops** -- every `CALLOTHER` the corpus reaches, by name. These are
//!    the OS and instruction-extension surface, and an unhandled one is
//!    answered with a zero.
//!
//! Code is found by recursive descent from the entry point and the loader's
//! function symbols, following direct branches and calls -- the same shape the
//! emulator's own translation cache would cover. A linear sweep of `.text`
//! would decode padding and jump tables as instructions and report their
//! nonsense as a decode failure.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use fission_emulator::jit::compiler::{GuestInsn, JitCompiler};
use fission_emulator::pcode::spaces::SpaceLayout;
use fission_loader::loader::LoadedBinary;
use fission_pcode::ir::{PcodeOp, PcodeOpcode};
use fission_sleigh::runtime::{PackedContextOverride, RuntimeSleighFrontend};

/// Instructions to translate per binary.
///
/// Coverage is about breadth, not depth: a second pass through `bash`'s parser
/// finds no opcode the first pass missed, while the next binary might. Raise it
/// with `TRANSLATE_BUDGET` when chasing one binary's tail.
fn budget() -> u64 {
    std::env::var("TRANSLATE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000)
}

/// Which tree to translate.
///
/// The benchmark corpus by default. `TRANSLATE_ROOT` points it somewhere else
/// -- `corpus/dev` when the question is about an architecture the benchmark
/// corpus does not carry, which aarch64 is. Translation is safe on any of
/// them; only *execution* is restricted to what we compiled ourselves.
fn corpus_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("TRANSLATE_ROOT") {
        let path = PathBuf::from(root);
        return path.is_dir().then_some(path);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/decbench-data/binaries");
    path.is_dir().then_some(path)
}

#[derive(Default)]
struct Stats {
    binaries: usize,
    /// Binaries that produced no instruction at all.
    dead: Vec<String>,
    blocks: u64,
    instructions: u64,
    /// Instructions with a varnode wider than eight bytes.
    ///
    /// Nothing else here counts these, because the opcodes are ordinary
    /// `COPY`, `LOAD`, `INT_XOR` -- it is the *width* that is the question, so
    /// they compile cleanly whether or not anything executes them correctly.
    /// That is why the column exists.
    ///
    /// Sixteen-byte integer ops are executed properly now, in `u128`, by both
    /// engines. What is still approximate is the ten-byte group -- x87's
    /// 80-bit extended precision -- and anything above sixteen, where a YMM's
    /// thirty-two bytes move a byte at a time and nothing does arithmetic on
    /// them. Splitting the count by width, below, says which is which.
    wide_insns: u64,
    /// Which p-code ops those wide instructions are made of, by opcode and
    /// varnode width. This is the work queue for 128-bit semantics: "implement
    /// SIMD" is not a task, and the corpus says which operations actually
    /// occur.
    wide_ops: BTreeMap<String, u64>,
    /// Wide instructions bucketed by their widest varnode. 10 bytes is x87's
    /// 80-bit extended precision; 16 is SSE, or a 128-bit integer result.
    wide_by_width: BTreeMap<u32, u64>,
    decode_errors: BTreeMap<String, u64>,
    compile_errors: BTreeMap<String, u64>,
    unimplemented: BTreeMap<String, u64>,
    userops: BTreeMap<String, u64>,
    /// Decode failures per binary, so a language-wide rate can be traced to
    /// the images that produce it. A rate is not a bug report; a filename is.
    worst: Vec<(u64, u64, String)>,
}

impl Stats {
    fn merge(&mut self, other: Stats) {
        self.binaries += other.binaries;
        self.dead.extend(other.dead);
        self.blocks += other.blocks;
        self.wide_insns += other.wide_insns;
        for (k, v) in other.wide_ops {
            *self.wide_ops.entry(k).or_default() += v;
        }
        for (k, v) in other.wide_by_width {
            *self.wide_by_width.entry(k).or_default() += v;
        }
        self.instructions += other.instructions;
        for (k, v) in other.decode_errors {
            *self.decode_errors.entry(k).or_default() += v;
        }
        for (k, v) in other.compile_errors {
            *self.compile_errors.entry(k).or_default() += v;
        }
        for (k, v) in other.unimplemented {
            *self.unimplemented.entry(k).or_default() += v;
        }
        for (k, v) in other.userops {
            *self.userops.entry(k).or_default() += v;
        }
        self.worst.extend(other.worst);
    }
}

/// A message with the addresses taken out, so ten thousand failures at ten
/// thousand addresses group into one line.
fn shape(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '0' && chars.peek() == Some(&'x') {
            chars.next();
            while chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                chars.next();
            }
            out.push_str("0x…");
        } else {
            out.push(c);
        }
    }
    out.chars().take(110).collect()
}

/// Is this an absolute branch destination rather than a p-code-relative one?
fn absolute_target(op: &PcodeOp) -> Option<u64> {
    let dest = op.inputs.first()?;
    (!dest.is_constant && dest.space_id != 0).then_some(dest.offset)
}

fn terminates(ops: &[PcodeOp]) -> bool {
    ops.iter().any(|op| match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => absolute_target(op).is_some(),
        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::Return | PcodeOpcode::BranchInd => {
            true
        }
        _ => false,
    })
}

fn translate(path: &Path, stats: &mut Stats) {
    let Ok(binary) = LoadedBinary::from_file(path) else {
        return;
    };
    let Some(load_spec) = binary.load_spec().cloned() else {
        return;
    };
    let Ok(frontends) = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
    else {
        return;
    };
    let Some(sleigh) = frontends.into_iter().next() else {
        return;
    };
    let layout = sleigh
        .compiled_frontend()
        .map(SpaceLayout::from_compiled)
        .unwrap_or_default();
    let Ok(mut jit) = JitCompiler::new() else {
        return;
    };
    // The call-out table a run would own. Nothing executes here, so it is only
    // somewhere for the compiler to record what it would have called.
    let mut wide_ops: Vec<PcodeOp> = Vec::new();
    // The same primitive the decompiler uses, not a second copy of the rule:
    // a stripped Cortex-M image is all Thumb and all even-addressed, and
    // decoding it as ARM produces plausible nonsense rather than an error.
    let context: Option<PackedContextOverride> =
        fission_static::analysis::function_discovery::decode_context_for_address(
            &binary, &sleigh, None,
        );

    stats.binaries += 1;
    let name = path
        .strip_prefix(corpus_root().unwrap_or_default())
        .unwrap_or(path)
        .display()
        .to_string();

    let inner = binary.inner();
    let executable: Vec<(u64, u64)> = inner
        .sections
        .iter()
        .filter(|s| s.is_executable && s.virtual_size > 0)
        .map(|s| (s.virtual_address, s.virtual_address + s.virtual_size))
        .collect();
    let in_code = |addr: u64| executable.iter().any(|(lo, hi)| addr >= *lo && addr < *hi);

    // Seeds: the entry point and every function the loader believes is code.
    // Imports are stubs into a table that is not there in a file we never map.
    let mut queue: VecDeque<u64> = VecDeque::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let push = |queue: &mut VecDeque<u64>, seen: &mut HashSet<u64>, addr: u64| {
        if in_code(addr) && seen.insert(addr) {
            queue.push_back(addr);
        }
    };
    push(&mut queue, &mut seen, inner.entry_point);
    for f in &inner.functions {
        if !f.is_import {
            push(&mut queue, &mut seen, f.address);
        }
    }

    let budget = budget();
    let mut instructions = 0u64;

    while let Some(entry) = queue.pop_front() {
        if instructions >= budget {
            break;
        }
        let mut insns: Vec<GuestInsn> = Vec::new();
        let mut pc = entry;

        // One translation block: instructions until something leaves it.
        for _ in 0..256 {
            if !in_code(pc) || instructions >= budget {
                break;
            }
            let Some(bytes) = binary.view_bytes(pc, 16) else {
                break;
            };
            if bytes.is_empty() {
                break;
            }
            match sleigh.decode_and_lift_with_context_override(bytes, pc, context) {
                Ok((ops, len, details)) => {
                    instructions += 1;
                    if ops.iter().any(|op| {
                        op.output.as_ref().is_some_and(|v| v.size > 8)
                            || op.inputs.iter().any(|v| v.size > 8)
                    }) {
                        stats.wide_insns += 1;
                        let widest = ops
                            .iter()
                            .flat_map(|op| {
                                op.output
                                    .as_ref()
                                    .map(|v| v.size)
                                    .into_iter()
                                    .chain(op.inputs.iter().map(|v| v.size))
                            })
                            .max()
                            .unwrap_or(0);
                        *stats.wide_by_width.entry(widest).or_default() += 1;
                        for op in &ops {
                            let width = op
                                .output
                                .as_ref()
                                .map(|v| v.size)
                                .into_iter()
                                .chain(op.inputs.iter().map(|v| v.size))
                                .max()
                                .unwrap_or(0);
                            if width > 8 {
                                *stats
                                    .wide_ops
                                    .entry(format!("{:?}/{}B", op.opcode, width))
                                    .or_default() += 1;
                            }
                        }
                    }
                    for op in &ops {
                        if op.opcode == PcodeOpcode::CallOther {
                            let id = op.inputs.first().map_or(-1, |v| v.constant_val) as u32;
                            let named = details
                                .userops
                                .get(&id)
                                .cloned()
                                .unwrap_or_else(|| format!("userop_{id}"));
                            *stats.userops.entry(named).or_default() += 1;
                        }
                        // A direct call or branch is another place to start.
                        if matches!(
                            op.opcode,
                            PcodeOpcode::Call | PcodeOpcode::Branch | PcodeOpcode::CBranch
                        ) && let Some(target) = absolute_target(op)
                        {
                            push(&mut queue, &mut seen, target);
                        }
                    }
                    let done = terminates(&ops);
                    insns.push(GuestInsn {
                        pc,
                        len: len as u32,
                        ops,
                    });
                    if len == 0 {
                        break;
                    }
                    pc = pc.wrapping_add(len);
                    if done {
                        break;
                    }
                }
                Err(e) => {
                    *stats
                        .decode_errors
                        .entry(shape(&format!("{e:#}")))
                        .or_default() += 1;
                    break;
                }
            }
        }

        if insns.is_empty() {
            continue;
        }
        stats.blocks += 1;
        // Compiling is not executing. The pointer is dropped on the next line.
        if let Err(e) =
            jit.compile_translation_block(&insns, layout.register, layout.unique, &mut wide_ops)
        {
            *stats
                .compile_errors
                .entry(shape(&format!("{e:#}")))
                .or_default() += 1;
        }
    }

    for (op, n) in std::mem::take(&mut jit.unimplemented_ops) {
        *stats.unimplemented.entry(op).or_default() += n;
    }
    stats.instructions += instructions;
    let failures: u64 = stats.decode_errors.values().sum();
    if failures > 0 {
        stats.worst.push((failures, instructions, name.clone()));
    }
    if instructions == 0 {
        stats.dead.push(name);
    }
}

fn language_of(path: &Path) -> String {
    LoadedBinary::from_file(path)
        .ok()
        .and_then(|b| {
            b.load_spec()
                .map(|s| s.pair.language_id.as_str().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

#[test]
#[ignore = "a measurement over the whole benchmark corpus, not an assertion"]
fn how_much_of_the_benchmark_corpus_translates() {
    let Some(root) = corpus_root() else {
        eprintln!("skipping: DecBench corpus not present");
        return;
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    eprintln!("{} files under {}", files.len(), root.display());

    let started = std::time::Instant::now();
    let mut by_language: BTreeMap<String, Stats> = BTreeMap::new();
    for (i, path) in files.iter().enumerate() {
        let language = language_of(path);
        let mut stats = Stats::default();
        translate(path, &mut stats);
        by_language.entry(language).or_default().merge(stats);
        if (i + 1) % 100 == 0 {
            eprintln!("  ... {} of {}", i + 1, files.len());
        }
    }

    let mut total = Stats::default();
    eprintln!("\n=== by language ===");
    for (language, stats) in &by_language {
        eprintln!(
            "{language:<28} {:>4} binaries  {:>9} insns  {:>7} blocks  \
             decode-fail {:>4}  compile-fail {:>3}  dropped-ops {:>4}  \
             >8B-varnode {:>7} ({:.2}%)",
            stats.binaries,
            stats.instructions,
            stats.blocks,
            stats.decode_errors.values().sum::<u64>(),
            stats.compile_errors.values().sum::<u64>(),
            stats.unimplemented.values().sum::<u64>(),
            stats.wide_insns,
            stats.wide_insns as f64 / stats.instructions.max(1) as f64 * 100.0,
        );
    }
    for stats in by_language.into_values() {
        total.merge(stats);
    }

    eprintln!(
        "\n=== total ===\n{} binaries, {} instructions in {} blocks, {:.1}s",
        total.binaries,
        total.instructions,
        total.blocks,
        started.elapsed().as_secs_f64()
    );

    let report = |title: &str, map: &BTreeMap<String, u64>, n: usize| {
        if map.is_empty() {
            eprintln!("\n{title}: none");
            return;
        }
        let mut ranked: Vec<_> = map.iter().collect();
        ranked.sort_by_key(|(name, count)| (std::cmp::Reverse(**count), (*name).clone()));
        eprintln!("\n{title} ({} distinct):", map.len());
        for (name, count) in ranked.into_iter().take(n) {
            eprintln!("  {count:>8}  {name}");
        }
    };
    report("opcodes lowered to nothing", &total.unimplemented, 20);
    report(
        "p-code ops on varnodes wider than 8 bytes",
        &total.wide_ops,
        30,
    );
    eprintln!("\nwide instructions by widest varnode:");
    for (width, count) in &total.wide_by_width {
        eprintln!(
            "  {width:>3} bytes  {count:>8}  ({:.2}% of all instructions)",
            *count as f64 / total.instructions.max(1) as f64 * 100.0
        );
    }
    report("decode failures", &total.decode_errors, 15);
    report("compile failures", &total.compile_errors, 15);
    // Split the userops the corpus reaches by whether anything answers them.
    // A name answered as processor semantics is settled; the rest are the OS
    // and instruction-extension surface still to be written, and that list is
    // the point of the report.
    let (settled, open): (BTreeMap<_, _>, BTreeMap<_, _>) = total
        .userops
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .partition(|(name, _)| {
            fission_emulator::os::env::classify_processor_userop(name).is_some()
                || fission_emulator::os::env::is_syscall_userop(name)
        });
    report(
        "userops answered as processor or kernel entry",
        &settled,
        14,
    );
    report("userops nothing answers yet", &open, 30);

    if !total.worst.is_empty() {
        total
            .worst
            .sort_by_key(|(fails, _, _)| std::cmp::Reverse(*fails));
        eprintln!(
            "\nbinaries by decode failure ({} of {} had any):",
            total.worst.len(),
            total.binaries
        );
        for (fails, ok, name) in total.worst.iter().take(15) {
            let rate = *fails as f64 / (*fails + *ok).max(1) as f64 * 100.0;
            eprintln!("  {fails:>6} failed / {ok:>6} decoded  ({rate:>4.1}%)  {name}");
        }
    }

    if !total.dead.is_empty() {
        eprintln!(
            "\n{} binaries produced no instruction at all:",
            total.dead.len()
        );
        for name in total.dead.iter().take(15) {
            eprintln!("  {name}");
        }
    }
}
