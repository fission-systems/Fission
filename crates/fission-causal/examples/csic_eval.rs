//! P1 evaluation harness: every controlled case through CSIC and the
//! baselines.
//!
//! ```text
//! cargo run --profile quick-release -p fission-causal --example csic_eval -- \
//!     [--out report.json] [--case NAME]
//! ```
//!
//! Without `--case`, each case runs in its own child process so its peak
//! resident memory is its own. Output: a Markdown table on stdout and the
//! full JSON (every candidate and verdict) at `--out`.

use std::process::Command;

use fission_causal::eval::{self, Evaluation};
use fission_causal::runner::Config;
use serde::Serialize;

#[derive(Serialize, serde::Deserialize)]
struct Child {
    evaluation: serde_json::Value,
    max_rss_bytes: u64,
}

fn max_rss_bytes() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: getrusage writes into the struct we own.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") { raw } else { raw * 1024 }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let config = Config::default();
    if let Some(name) = arg("--case") {
        let spec = eval::cases()
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| anyhow::anyhow!("no case {name}"))?;
        let evaluation = eval::evaluate(&spec, &config)?;
        let child = Child {
            evaluation: serde_json::to_value(&evaluation)?,
            max_rss_bytes: max_rss_bytes(),
        };
        println!("{}", serde_json::to_string(&child)?);
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let mut rows = Vec::new();
    for spec in eval::cases() {
        let output = Command::new(&exe).args(["--case", spec.name]).output()?;
        if !output.status.success() {
            eprintln!(
                "{}: failed: {}",
                spec.name,
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .filter(|l| !l.contains(" INFO ") && !l.contains(" WARN "))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            continue;
        }
        let line = String::from_utf8_lossy(&output.stdout);
        let child: Child = serde_json::from_str(line.trim())?;
        let evaluation: Evaluation = serde_json::from_value(child.evaluation.clone())
            .map_err(|e| anyhow::anyhow!("{}: {e}", spec.name))?;
        eprintln!("{}: done", spec.name);
        rows.push((evaluation, child));
    }
    print_table(&rows);
    if let Some(out) = arg("--out") {
        let all: Vec<&Child> = rows.iter().map(|(_, c)| c).collect();
        std::fs::write(out, serde_json::to_string_pretty(&all)?)?;
    }
    Ok(())
}

fn cell(s: &eval::Score) -> String {
    format!("{}/{}/{}", s.true_pos, s.false_pos, s.false_neg)
}

fn print_table(rows: &[(Evaluation, Child)]) {
    println!(
        "| case | truth | SS nodes | SS TP/FP/FN | DS nodes | DS TP/FP/FN | DT-data TP/FP/FN | DT-ctl TP/FP/FN | CSIC cands | CSIC TP/FP/FN | joint | refused | oracles | ckpt capture ms | restore us | replay-to-step us | fresh emulator ms | oracle us | peak RSS MiB | total ms | exhaustive TP/FP/FN |"
    );
    println!("|{}", "---|".repeat(21));
    for (e, c) in rows {
        let r = &e.report;
        let t = r.timing.clone().unwrap_or_default();
        let truth = if e.truth.refuse {
            "refuse".to_string()
        } else if !e.truth.joint.is_empty() {
            format!("joint{:?}", e.truth.joint)
        } else {
            format!("{:?}", e.truth.causes)
        };
        let joint = match &r.joint {
            fission_causal::contract::JointStatus::NotNeeded => "-".to_string(),
            fission_causal::contract::JointStatus::JointWitness { input_offsets, .. } => {
                format!("pair{input_offsets:?}")
            }
            fission_causal::contract::JointStatus::NoWitnessWithinBudget { pairs_tried } => {
                format!("none({pairs_tried})")
            }
            fission_causal::contract::JointStatus::Refused => "refused".to_string(),
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.0} | {:.0} | {:.1} | {:.0} | {:.1} | {:.0} | {} |",
            e.name,
            truth,
            e.static_slice.nodes,
            cell(&e.static_slice),
            e.dynamic_slice.nodes,
            cell(&e.dynamic_slice),
            cell(&e.taint_data),
            cell(&e.taint_data_control),
            e.csic.nodes,
            cell(&e.csic),
            joint,
            if e.refused { "yes" } else { "no" },
            r.counts.oracle_executions,
            t.checkpoint_capture_us as f64 / 1000.0,
            t.checkpoint_restore_us_mean,
            t.replay_to_step_us_mean,
            t.fresh_emulator_us_mean / 1000.0,
            t.oracle_us_mean,
            c.max_rss_bytes as f64 / (1024.0 * 1024.0),
            t.total_us as f64 / 1000.0,
            cell(&e.exhaustive),
        );
    }
}
