//! Single instructions on the real CPU against Fission's lift and interpreter.
//!
//! # Why
//!
//! Fission's emulator and decompiler share one lift: the SLEIGH specs it
//! compiles from Ghidra. When a spec is wrong, everything built on it agrees
//! on the wrong answer -- AArch64 FCVT{A,M,N,P} all lifted as truncation
//! (#834) and every FRINT mode wrote integer bits into a float register
//! (#838), and neither Ghidra nor `fission_cli verify` could see it. The
//! processor does not read SLEIGH, so it can (ADR 0017, decision 2.1).
//!
//! # How
//!
//! For each case: build a random but valid state (GPRs, flags, vector
//! registers, a 16 KiB buffer the pointers aim into), lift the bytes with
//! [`RuntimeSleighFrontend`](fission_sleigh::runtime::RuntimeSleighFrontend)
//! and run the p-code in Fission's interpreter, run the same bytes on the host
//! CPU from the same state (see `native.rs`), and compare every register,
//! flag, vector register and buffer byte. Results the architecture manual
//! leaves undefined are masked per instruction, with the manual section
//! cited at the mask.
//!
//! Two sources of cases:
//! - **curated**: hand-picked instructions (integer ALU and flags, shifts,
//!   multiply/divide, bit scans, conditional moves, SSE/NEON conversions,
//!   rounding, min/max, shuffles) each run from `STATES` random states;
//! - **random**: random byte strings the SLEIGH decoder accepts, minus those
//!   that branch, call into the host, or touch state the harness cannot share.
//!
//! # The gate
//!
//! `baseline.txt` lists the known divergences, each with an issue. The test
//! fails on a divergence not listed there, and on a listed one that no longer
//! reproduces, so the list cannot rot. Every failure prints a one-line
//! reproducer (seed, case, bytes, input state, both results).
//!
//! Knobs (environment): `FISSION_NATIVE_DIFF_SEED`, `FISSION_NATIVE_DIFF_STATES`
//! (states per curated instruction), `FISSION_NATIVE_DIFF_RANDOM` (random
//! cases), `FISSION_NATIVE_DIFF_ONLY` (run only case ids containing this),
//! `FISSION_NATIVE_DIFF_CASE` (run exactly one case id, e.g. `r:52993` or
//! `c:add eax, ecx#100`, whatever the bounds; a reproducer prints it),
//! `FISSION_NATIVE_DIFF_SHOW` (reproducers printed per new divergence),
//! `FISSION_NATIVE_DIFF_PCODE` (print each reproducer's lifted p-code).

#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(any(target_os = "linux", target_os = "macos"), target_arch = "aarch64")
))]

mod fission;
mod model;
mod rng;
mod target_aarch64;
mod target_x86_64;

mod native;

use std::collections::{BTreeMap, BTreeSet};

use fission::{FissionFail, FissionMachine, Lifted};
use fission_pcode_ir::ir::PcodeOp;
use model::{CpuState, FieldDiff, Isa, Mask};
use rng::Rng;

pub struct Curated {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// One ISA's half of the harness.
pub struct Target {
    pub isa: &'static Isa,
    /// A fixture of this ISA; only its language id is used.
    pub fixture: &'static str,
    pub curated: fn() -> Vec<Curated>,
    pub state: fn(&mut Rng, Option<&str>) -> CpuState,
    pub random_insn: fn(&mut Rng) -> Vec<u8>,
    pub skip: fn(&[u8], &Lifted, &FissionMachine) -> Option<String>,
    /// Whether the instruction re-executes itself by design (`rep movsb`).
    pub repeats: fn(&[u8], &Lifted) -> bool,
    pub mask: fn(&[u8], &Lifted, &CpuState) -> Mask,
    pub extra: fn() -> Vec<(&'static str, u64)>,
}

const DEFAULT_SEED: u64 = 0x5EED_F155_10A0_0001;
const DEFAULT_STATES: u64 = 24;
const DEFAULT_RANDOM: u64 = 3000;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| {
            let v = v.trim();
            match v.strip_prefix("0x") {
                Some(h) => u64::from_str_radix(h, 16).ok(),
                None => v.parse().ok(),
            }
        })
        .unwrap_or(default)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone)]
struct Divergence {
    case_id: String,
    bytes: Vec<u8>,
    text: String,
    input: CpuState,
    diffs: Vec<FieldDiff>,
    /// The lifted p-code, one op per line.
    pcode: Vec<String>,
}

#[derive(Default)]
struct Stats {
    cases: u64,
    compared: u64,
    undecodable: u64,
    both_fault: u64,
    skipped: BTreeMap<String, u64>,
    native_only_fault: BTreeMap<String, (u64, String)>,
    unmodeled: BTreeMap<String, (u64, String)>,
    compared_mnemonics: BTreeSet<String>,
    curated_mnemonics: BTreeSet<String>,
    /// Curated instruction name -> why its cases did not compare (the last
    /// reason seen); removed once any of its cases compares.
    curated_uncompared: BTreeMap<String, String>,
    curated_compared: BTreeSet<String>,
    random_mnemonics: BTreeSet<String>,
    /// Mnemonic -> divergences.
    divergences: BTreeMap<String, Vec<Divergence>>,
}

/// The native side maps fixed addresses and installs process-wide signal
/// handlers, so harnesses never overlap (libtest runs tests on threads).
static EXCLUSIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Harness<'a> {
    target: &'a Target,
    fission: FissionMachine,
    native: native::NativeRunner,
    insn_pc: u64,
    extra: Vec<(&'static str, u64)>,
    seed: u64,
    // Last, so it is released after the native runner has been dropped.
    _exclusive: std::sync::MutexGuard<'static, ()>,
}

impl<'a> Harness<'a> {
    fn new(target: &'a Target, seed: u64) -> Self {
        let exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
        let native = native::NativeRunner::new();
        let (_, insn_pc, _, _) = native.page_for(&[]);
        let fission = FissionMachine::new(
            target.isa,
            target.fixture,
            native::buf_addr(),
            native.code_addr,
            native::REGION_LEN,
        );
        Self {
            target,
            fission,
            native,
            insn_pc,
            extra: (target.extra)(),
            seed,
            _exclusive: exclusive,
        }
    }

    /// Run one case. `curated` names the instruction for curated cases.
    fn run_case(
        &mut self,
        stats: &mut Stats,
        case_id: &str,
        raw: &[u8],
        input: &CpuState,
        curated: bool,
        mutate: Option<fn(&mut Vec<PcodeOp>)>,
    ) {
        stats.cases += 1;
        let curated_name = case_id
            .strip_prefix("c:")
            .and_then(|c| c.rsplit_once('#'))
            .map(|(name, _)| name.to_string());
        let note = |stats: &mut Stats, why: &str| {
            if let Some(name) = &curated_name {
                if !stats.curated_compared.contains(name) {
                    stats
                        .curated_uncompared
                        .insert(name.clone(), why.to_string());
                }
            }
        };
        let Some(mut lifted) = self.fission.lift(raw, self.insn_pc) else {
            if curated {
                stats
                    .divergences
                    .entry(format!("<undecodable {}>", hex(raw)))
                    .or_default()
                    .push(Divergence {
                        case_id: case_id.to_string(),
                        bytes: raw.to_vec(),
                        text: String::new(),
                        input: input.clone(),
                        pcode: Vec::new(),
                        diffs: vec![FieldDiff {
                            field: "decode".into(),
                            input: String::new(),
                            native: "decodes".into(),
                            fission: "no decode".into(),
                        }],
                    });
            } else {
                stats.undecodable += 1;
            }
            return;
        };
        if let Some(m) = mutate {
            m(&mut lifted.ops);
        }
        let bytes = &raw[..lifted.len];
        let repeats = (self.target.repeats)(bytes, &lifted);
        let skip = fission::control_flow_skip(&lifted, self.insn_pc, &self.fission, repeats)
            .or_else(|| (self.target.skip)(bytes, &lifted, &self.fission));
        if let Some(reason) = skip {
            if std::env::var_os("FISSION_NATIVE_DIFF_PCODE").is_some() {
                println!("   skipped {case_id} ({reason}): {}", lifted.text);
                for op in &lifted.ops {
                    println!("       {op:?}");
                }
            }
            note(stats, &format!("skipped: {reason}"));
            *stats.skipped.entry(reason).or_default() += 1;
            return;
        }
        let (page, insn_pc, end_pc, exit_pc) = self.native.page_for(bytes);
        debug_assert_eq!(insn_pc, self.insn_pc);
        let fis = self
            .fission
            .run(&lifted, insn_pc, &page, input, &self.extra);
        // Fission faulted outside the two shared regions: the CPU would touch
        // the same address in the host's own memory, where it may well be
        // mapped (a heap, a library) and a store would corrupt this process.
        // Neither side's answer means anything there, so it is not run.
        if let Err(FissionFail::Fault(msg)) = &fis {
            let shared = |a: u64| {
                (native::buf_addr()..native::buf_addr() + model::BUF_LEN as u64).contains(&a)
                    || (self.native.code_addr..self.native.code_addr + native::REGION_LEN as u64)
                        .contains(&a)
            };
            if fault_address(msg).is_none_or(|a| !shared(a)) {
                note(stats, "Fission faulted outside the buffer");
                stats.both_fault += 1;
                return;
            }
        }
        if std::env::var_os("FISSION_NATIVE_DIFF_TRACE").is_some() {
            // Before the native run, so a case that takes the host down is named.
            eprintln!("native {case_id} {} {}", hex(bytes), lifted.text);
        }
        let nat = self.native.run(&page, end_pc, exit_pc, input);
        let mn = lifted.mnemonic.clone();
        let diverge = |stats: &mut Stats, diffs: Vec<FieldDiff>| {
            stats
                .divergences
                .entry(mn.clone())
                .or_default()
                .push(Divergence {
                    case_id: case_id.to_string(),
                    bytes: bytes.to_vec(),
                    text: lifted.text.clone(),
                    input: input.clone(),
                    diffs,
                    pcode: lifted.ops.iter().map(|op| format!("{op:?}")).collect(),
                });
        };
        match (fis, nat) {
            (Ok(f), Ok(n)) => {
                if let Some(name) = &curated_name {
                    stats.curated_uncompared.remove(name);
                    stats.curated_compared.insert(name.clone());
                }
                stats.compared += 1;
                stats.compared_mnemonics.insert(mn.clone());
                if curated {
                    stats.curated_mnemonics.insert(mn.clone());
                } else {
                    stats.random_mnemonics.insert(mn.clone());
                }
                let mask = (self.target.mask)(bytes, &lifted, input);
                let mut diffs = model::compare(self.target.isa, mask, input, &n, &f);
                if half_precision(&lifted.ops) {
                    for d in &mut diffs {
                        d.field.push_str("~fp16");
                    }
                }
                if !diffs.is_empty() {
                    diverge(stats, diffs);
                }
            }
            (Err(FissionFail::Fault(_)), Err(_)) => {
                note(stats, "both faulted");
                stats.both_fault += 1;
            }
            (Err(FissionFail::Fault(msg)), Ok(_)) => diverge(
                stats,
                vec![FieldDiff {
                    field: "fault".into(),
                    input: String::new(),
                    native: "completes".into(),
                    fission: msg,
                }],
            ),
            (Ok(_), Err(fault)) => {
                note(stats, &format!("only the CPU faulted: {fault}"));
                let e = stats
                    .native_only_fault
                    .entry(mn.clone())
                    .or_insert((0, format!("{case_id}: {fault}")));
                e.0 += 1;
            }
            (Err(FissionFail::Panic(msg)), _) => diverge(
                stats,
                vec![FieldDiff {
                    field: "panic".into(),
                    input: String::new(),
                    native: "-".into(),
                    fission: msg,
                }],
            ),
            (Err(FissionFail::Unmodeled(msg)), _) => {
                note(stats, &format!("Fission could not execute: {msg}"));
                let e = stats
                    .unmodeled
                    .entry(mn.clone())
                    .or_insert((0, format!("{case_id}: {msg}")));
                e.0 += 1;
            }
        }
    }

    fn reproducer(&self, mnemonic: &str, d: &Divergence) -> String {
        let isa = self.target.isa;
        let mut state = String::new();
        for (i, name) in isa.gpr.iter().enumerate() {
            state.push_str(&format!("{name}={:#x} ", d.input.gpr[i]));
        }
        for (i, name) in isa.flags.iter().enumerate() {
            state.push_str(&format!("{name}={} ", d.input.flags[i]));
        }
        // The vector registers the instruction names (xmm1; v1, q1, d1, s1...).
        let named: BTreeSet<usize> = d
            .text
            .to_ascii_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter_map(|tok| {
                let digits = tok.trim_start_matches(|c: char| c.is_ascii_alphabetic());
                let prefix = &tok[..tok.len() - digits.len()];
                let vector = match isa.name {
                    "x86_64" => prefix == "xmm",
                    _ => matches!(prefix, "v" | "q" | "d" | "s" | "h" | "b"),
                };
                if vector {
                    digits.parse::<usize>().ok()
                } else {
                    None
                }
            })
            .filter(|&n| n < isa.vec.len())
            .collect();
        for n in named {
            state.push_str(&format!(
                "{}={:032x} ",
                isa.vec[n],
                u128::from_le_bytes(d.input.vec[n])
            ));
        }
        let diffs: Vec<String> = d
            .diffs
            .iter()
            .map(|f| {
                format!(
                    "{}: in={} native={} fission={}",
                    f.field, f.input, f.native, f.fission
                )
            })
            .collect();
        format!(
            "REPRO isa={} mnemonic={mnemonic} bytes={} text=\"{}\" seed={:#x} case=\"{}\" | {}| {} | rerun: FISSION_NATIVE_DIFF_SEED={:#x} FISSION_NATIVE_DIFF_CASE='{}' cargo nextest run -p fission-emulator --test native_differential --no-capture",
            isa.name,
            hex(&d.bytes),
            d.text,
            self.seed,
            d.case_id,
            state,
            diffs.join("; "),
            self.seed,
            d.case_id,
        )
    }
}

/// The page address in a page-fault message ("... at 0x74B234000 (Read)").
fn fault_address(msg: &str) -> Option<u64> {
    let hex = msg.split("at 0x").nth(1)?;
    let end = hex
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(hex.len());
    u64::from_str_radix(&hex[..end], 16).ok()
}

/// Whether the p-code does float arithmetic on two-byte (half-precision)
/// varnodes, which the evaluator does not implement (#1065).
fn half_precision(ops: &[PcodeOp]) -> bool {
    use fission_pcode_ir::ir::PcodeOpcode as O;
    ops.iter().any(|op| {
        matches!(
            op.opcode,
            O::FloatAdd
                | O::FloatSub
                | O::FloatMult
                | O::FloatDiv
                | O::FloatNeg
                | O::FloatAbs
                | O::FloatSqrt
                | O::FloatFloat2Float
                | O::FloatInt2Float
                | O::FloatTrunc
                | O::FloatCeil
                | O::FloatFloor
                | O::FloatRound
                | O::FloatEqual
                | O::FloatNotEqual
                | O::FloatLess
                | O::FloatLessEqual
                | O::FloatNan
        ) && (op.output.as_ref().is_some_and(|v| v.size == 2)
            || op.inputs.iter().any(|v| v.size == 2))
    })
}

// ── Baseline ────────────────────────────────────────────────────────────────

struct BaselineEntry {
    line: usize,
    isa: String,
    mnemonic: String,
    /// Fields allowed to diverge; `*` allows any.
    fields: Vec<String>,
    issue: String,
    /// Reproduced only at the nightly case count (`@nightly` in the reason).
    nightly: bool,
    /// Reproduces on some host CPUs and not others (`@host` in the reason):
    /// the hardware itself disagrees, so a run that does not reproduce it is
    /// not evidence it was fixed.
    host_dependent: bool,
}

/// The host CPU model, so a host-dependent result can be traced to the
/// machine that produced it (CI runners differ in vendor and generation).
fn host_cpu() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        if let Some(model) = info.lines().find_map(|l| {
            l.strip_prefix("model name")
                .map(|r| r.trim_start_matches([' ', '\t', ':']))
        }) {
            return model.to_string();
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
    {
        return String::from_utf8_lossy(&out.stdout).trim().to_string();
    }
    std::env::consts::ARCH.to_string()
}

/// The nightly workflow's case count (`native-differential.yml`).
const NIGHTLY_STATES: u64 = 128;
const NIGHTLY_RANDOM: u64 = 100_000;

fn load_baseline() -> Vec<BaselineEntry> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/native_differential/baseline.txt");
    let text = std::fs::read_to_string(&path).expect("baseline.txt");
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = l.split_whitespace().collect();
        assert!(
            parts.len() >= 5 && parts[3].starts_with('#'),
            "baseline.txt:{line_no}: expected `<isa> <MNEMONIC> <fields|*> #<issue> <reason>`"
        );
        out.push(BaselineEntry {
            line: line_no,
            isa: parts[0].to_string(),
            mnemonic: parts[1].to_string(),
            fields: parts[2].split(',').map(str::to_string).collect(),
            issue: parts[3].to_string(),
            nightly: parts[4..].contains(&"@nightly"),
            host_dependent: parts[4..].contains(&"@host"),
        });
    }
    out
}

/// `*` matches every mnemonic, `LDADD*` every one starting with `LDADD`.
fn mnemonic_matches(pattern: &str, mnemonic: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => mnemonic.starts_with(prefix),
        None => pattern == mnemonic,
    }
}

/// `*` covers any field; `~tag` covers any field carrying that tag (`~nan`:
/// both sides NaN, payloads differ; `~fp16`: the instruction computes on
/// half-precision floats).
fn covered(entry: &BaselineEntry, field: &str) -> bool {
    entry
        .fields
        .iter()
        .any(|f| f == "*" || f == field || (f.starts_with('~') && field.contains(f.as_str())))
}

// ── Driver ──────────────────────────────────────────────────────────────────

fn host_target() -> Option<&'static Target> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(&target_x86_64::TARGET)
    } else if cfg!(all(
        any(target_os = "linux", target_os = "macos"),
        target_arch = "aarch64"
    )) {
        Some(&target_aarch64::TARGET)
    } else {
        None
    }
}

fn run_all<'a>(
    target: &'a Target,
    seed: u64,
    states: u64,
    random: u64,
    only: Option<&str>,
    exact: Option<&str>,
) -> (Stats, Harness<'a>) {
    let mut h = Harness::new(target, seed);
    let mut stats = Stats::default();
    let isa = target.isa.name;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let curated = (target.curated)();
    if let Some(case_id) = exact {
        // One case by its id, whatever the case-count bounds: a nightly
        // reproducer (`r:52993`, `c:...#100`) must run at the PR defaults.
        let mut rng = Rng::for_case(seed, &format!("{isa}:{case_id}"), 0);
        if case_id
            .strip_prefix("r:")
            .is_some_and(|i| i.parse::<u64>().is_ok())
        {
            let bytes = (target.random_insn)(&mut rng);
            let input = (target.state)(&mut rng, None);
            h.run_case(&mut stats, case_id, &bytes, &input, false, None);
        } else if let Some((name, _)) = case_id
            .strip_prefix("c:")
            .and_then(|c| c.rsplit_once('#'))
            .filter(|(_, k)| k.parse::<u64>().is_ok())
        {
            if let Some(c) = curated.iter().find(|c| c.name == name) {
                let input = (target.state)(&mut rng, Some(&c.name));
                h.run_case(&mut stats, case_id, &c.bytes, &input, true, None);
            }
        }
        std::panic::set_hook(hook);
        return (stats, h);
    }
    for c in &curated {
        for k in 0..states {
            let case_id = format!("c:{}#{k}", c.name);
            if only.is_some_and(|o| !case_id.contains(o)) {
                continue;
            }
            let mut rng = Rng::for_case(seed, &format!("{isa}:{case_id}"), 0);
            let input = (target.state)(&mut rng, Some(&c.name));
            h.run_case(&mut stats, &case_id, &c.bytes, &input, true, None);
        }
    }
    for i in 0..random {
        let case_id = format!("r:{i}");
        if only.is_some_and(|o| case_id != o && !case_id.starts_with(&format!("{o}#"))) {
            continue;
        }
        let mut rng = Rng::for_case(seed, &format!("{isa}:{case_id}"), 0);
        let bytes = (target.random_insn)(&mut rng);
        let input = (target.state)(&mut rng, None);
        h.run_case(&mut stats, &case_id, &bytes, &input, false, None);
    }
    std::panic::set_hook(hook);
    (stats, h)
}

#[test]
fn native_differential() {
    let Some(target) = host_target() else {
        eprintln!("native_differential: no native runner for this host; skipped");
        return;
    };
    let seed = env_u64("FISSION_NATIVE_DIFF_SEED", DEFAULT_SEED);
    let states = env_u64("FISSION_NATIVE_DIFF_STATES", DEFAULT_STATES);
    let random = env_u64("FISSION_NATIVE_DIFF_RANDOM", DEFAULT_RANDOM);
    let only = std::env::var("FISSION_NATIVE_DIFF_ONLY").ok();
    let exact = std::env::var("FISSION_NATIVE_DIFF_CASE").ok();
    let isa = target.isa.name;
    let start = std::time::Instant::now();
    let curated_count = (target.curated)().len();
    let (stats, h) = run_all(
        target,
        seed,
        states,
        random,
        only.as_deref(),
        exact.as_deref(),
    );
    let elapsed = start.elapsed();

    println!(
        "== native differential: {isa}, seed {seed:#x}, host {}",
        host_cpu()
    );
    println!(
        "   curated: {curated_count} instructions x {states} states; random: {random} byte strings; {:.1}s",
        elapsed.as_secs_f64()
    );
    println!(
        "   cases {} | compared {} | both faulted {} | undecodable {} | skipped {}",
        stats.cases,
        stats.compared,
        stats.both_fault,
        stats.undecodable,
        stats.skipped.values().sum::<u64>()
    );
    println!(
        "   coverage: {} distinct mnemonics compared ({} curated, {} from random)",
        stats.compared_mnemonics.len(),
        stats.curated_mnemonics.len(),
        stats.random_mnemonics.len()
    );
    println!(
        "   curated instructions compared: {} of {curated_count}",
        stats.curated_compared.len()
    );
    if !stats.curated_uncompared.is_empty() {
        println!("   curated instructions never compared (the reason last seen):");
        for (name, why) in &stats.curated_uncompared {
            println!("     {name}: {why}");
        }
    }
    if std::env::var_os("FISSION_NATIVE_DIFF_VERBOSE").is_some() {
        println!("   mnemonics: {:?}", stats.compared_mnemonics);
    }
    let mut skipped: Vec<_> = stats.skipped.iter().collect();
    skipped.sort_by(|a, b| b.1.cmp(a.1));
    println!(
        "   skipped by reason: {}",
        skipped
            .iter()
            .take(25)
            .map(|(r, n)| format!("{r}={n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !stats.unmodeled.is_empty() {
        println!("   Fission could not execute (reported, not gated):");
        for (mn, (n, first)) in &stats.unmodeled {
            println!("     {mn}: {n} ({first})");
        }
    }
    if !stats.native_only_fault.is_empty() {
        println!(
            "   only the CPU faulted (unsupported/privileged on this host, or undefined input):"
        );
        for (mn, (n, first)) in &stats.native_only_fault {
            println!("     {mn}: {n} ({first})");
        }
    }

    let baseline: Vec<BaselineEntry> = load_baseline()
        .into_iter()
        .filter(|e| e.isa == isa)
        .collect();
    let mut used = vec![false; baseline.len()];
    let mut failures = Vec::new();
    for (mn, divs) in &stats.divergences {
        let fields: BTreeSet<&str> = divs
            .iter()
            .flat_map(|d| d.diffs.iter().map(|f| f.field.as_str()))
            .collect();
        let mut issues = BTreeSet::new();
        let unexplained: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| {
                let mut hit = false;
                for (i, e) in baseline.iter().enumerate() {
                    if mnemonic_matches(&e.mnemonic, mn) && covered(e, f) {
                        used[i] = true;
                        issues.insert(e.issue.as_str());
                        hit = true;
                    }
                }
                !hit
            })
            .collect();
        let status = if unexplained.is_empty() {
            format!("known {}", issues.into_iter().collect::<Vec<_>>().join(","))
        } else {
            "NEW".to_string()
        };
        println!(
            "   divergence [{status}] {mn}: {} case(s), fields {:?}",
            divs.len(),
            fields
        );
        let show = env_u64("FISSION_NATIVE_DIFF_SHOW", 3) as usize;
        let show = if unexplained.is_empty() {
            show.min(1)
        } else {
            show
        };
        for d in divs.iter().take(show) {
            println!("     {}", h.reproducer(mn, d));
            if std::env::var_os("FISSION_NATIVE_DIFF_PCODE").is_some() {
                for op in &d.pcode {
                    println!("       {op}");
                }
            }
        }
        if !unexplained.is_empty() {
            failures.push(format!("{mn}: new divergence in {unexplained:?}"));
        }
    }
    // An `@nightly` entry is rare enough that only the nightly case count is
    // sure to hit it; smaller runs do not call it stale.
    let nightly_size = states >= NIGHTLY_STATES && random >= NIGHTLY_RANDOM;
    // A filter that selects nothing would pass while checking nothing (a
    // misspelt id, or a reproducer past the case-count bounds).
    if (only.is_some() || exact.is_some()) && stats.cases == 0 {
        failures.push(format!(
            "FISSION_NATIVE_DIFF_ONLY={only:?} / FISSION_NATIVE_DIFF_CASE={exact:?} selected no case"
        ));
    }
    if only.is_none() && exact.is_none() {
        for (i, e) in baseline.iter().enumerate() {
            if !used[i] && !e.host_dependent && (nightly_size || !e.nightly) {
                failures.push(format!(
                    "baseline.txt:{}: {} {} {} no longer reproduces; remove it (and close the issue if fixed)",
                    e.line, e.isa, e.mnemonic, e.issue
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "native differential ({isa}) failed:\n  {}",
        failures.join("\n  ")
    );
}

/// The harness has to be able to fail. Corrupt one p-code op of an `add`
/// (INT_ADD becomes INT_SUB) and check that the comparison sees it.
#[test]
fn a_corrupted_lift_is_caught() {
    let Some(target) = host_target() else {
        return;
    };
    let (name, bytes): (&str, &[u8]) = match target.isa.name {
        // LEA: an INT_ADD with no flags, so nothing else can differ.
        "x86_64" => ("lea rax, [rax+rcx]", &[0x48, 0x8D, 0x04, 0x08]),
        _ => ("add x0, x1, x2", &[0x20, 0x00, 0x02, 0x8B]),
    };
    let mut h = Harness::new(target, DEFAULT_SEED);
    let flip: fn(&mut Vec<PcodeOp>) = |ops| {
        let op = ops
            .iter_mut()
            .find(|op| op.opcode == fission_pcode_ir::ir::PcodeOpcode::IntAdd)
            .expect("an INT_ADD to corrupt");
        op.opcode = fission_pcode_ir::ir::PcodeOpcode::IntSub;
    };
    let mut clean = Stats::default();
    let mut corrupt = Stats::default();
    for k in 0..8 {
        let mut rng = Rng::for_case(DEFAULT_SEED, &format!("control:{k}"), 0);
        let mut input = (target.state)(&mut rng, Some(name));
        // A nonzero second operand, so add and sub differ.
        input.gpr[if target.isa.name == "x86_64" { 1 } else { 2 }] |= 1;
        h.run_case(
            &mut clean,
            &format!("control#{k}"),
            bytes,
            &input,
            true,
            None,
        );
        h.run_case(
            &mut corrupt,
            &format!("control#{k}"),
            bytes,
            &input,
            true,
            Some(flip),
        );
    }
    assert_eq!(clean.compared, 8, "the clean add must run on both sides");
    let clean_diffs: Vec<String> = clean
        .divergences
        .iter()
        .flat_map(|(mn, ds)| ds.iter().map(|d| h.reproducer(mn, d)))
        .collect();
    assert!(
        clean_diffs.is_empty(),
        "the clean add must agree: {clean_diffs:#?}"
    );
    let caught: usize = corrupt.divergences.values().map(Vec::len).sum();
    assert_eq!(caught, 8, "every corrupted case must diverge");
}

/// The watchdog's SIGALRM is process-directed, so the kernel may hand it to
/// any thread. An instruction that never finishes (`jmp $` / `b .`) must
/// still be stopped on the thread running the stub, with every other thread
/// left alone, and that thread must run the next case normally.
#[test]
fn a_looping_instruction_is_stopped_on_its_own_thread() {
    let Some(target) = host_target() else {
        return;
    };
    let _exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
    let (looping, nop): (&[u8], &[u8]) = match target.isa.name {
        "x86_64" => (&[0xEB, 0xFE], &[0x90]),
        _ => (&[0x00, 0x00, 0x00, 0x14], &[0x1F, 0x20, 0x03, 0xD5]),
    };
    // Two busy threads that do not block SIGALRM, so delivery elsewhere
    // than the stub thread is likely.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let spinners: Vec<_> = (0..2)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut n = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    n = n.wrapping_add(1);
                    std::hint::black_box(n);
                }
            })
        })
        .collect();
    let stub_thread = std::thread::spawn(move || {
        let mut runner = native::NativeRunner::new();
        let mut rng = Rng::new(7);
        let mut outcomes = Vec::new();
        for _ in 0..4 {
            let input = (target.state)(&mut rng, Some("loop"));
            let (page, _, end, exit) = runner.page_for(looping);
            outcomes.push(match runner.run(&page, end, exit, &input) {
                Err(native::NativeFault::Signal { signal, .. }) => signal,
                Ok(_) => 0,
            });
        }
        let input = (target.state)(&mut rng, Some("nop"));
        let (page, _, end, exit) = runner.page_for(nop);
        let after = runner.run(&page, end, exit, &input).is_ok();
        (outcomes, after)
    });
    let (outcomes, after) = stub_thread.join().expect("the stub thread survives");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for s in spinners {
        s.join().expect("the other threads are untouched");
    }
    println!(
        "watchdog: {outcomes:?}, {} signal(s) forwarded from another thread",
        native::forwarded_watchdog_signals()
    );
    assert_eq!(
        outcomes,
        vec![libc::SIGALRM; 4],
        "each loop ends by the watchdog on its own thread"
    );
    assert!(after, "the stub thread runs the next case normally");
}
