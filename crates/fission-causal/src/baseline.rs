//! Baselines CSIC is compared against.
//!
//! - **Dynamic taint** ([`dynamic_taint`]): Fission's own per-byte taint
//!   engine (`fission_emulator::taint`), one label per input byte, read at
//!   the sink criterion. Reported twice: direct-data provenance only (the
//!   classic dynamic taint), and data plus Fission's bounded implicit-flow
//!   (control) provenance.
//! - **Dynamic slice** ([`crate::slice::backward`]): the input bytes the
//!   CSIC candidate generator's own slice reaches, before any
//!   intervention. Shows what interventions remove.
//! - **Static slice** ([`crate::static_slice`]): a pinned conservative
//!   static slice with no points-to analysis.

use std::collections::BTreeSet;

use anyhow::Result;
use fission_emulator::observe::ShadowMode;
use serde::Serialize;

use crate::machine::{Engine, Program, Run};
use crate::trace::{Access, Trace};

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaintResult {
    /// Input offsets in the sink criterion's direct-data taint.
    pub data: BTreeSet<usize>,
    /// ... plus control (implicit-flow) provenance.
    pub data_control: BTreeSet<usize>,
    /// Criterion bytes inspected.
    pub criterion_bytes: usize,
    /// Fission's control tracking dropped no scopes.
    pub control_complete: bool,
    /// Which engine carried the taint.
    pub engine: String,
}

/// Where to read taint: `(step, accesses)`, read just before `step` runs.
pub fn taint_criteria(trace: &Trace) -> Vec<(u64, Vec<Access>)> {
    let mut out = vec![(trace.sink_step, trace.criterion.clone())];
    // An event sink with no value to inspect ("the call happened") is read
    // at the branch that guards it -- the same dynamic control parent the
    // slicer follows.
    if trace.criterion.is_empty()
        && let Some(parent) = trace.step(trace.sink_step).and_then(|s| s.cd_parent)
        && let Some(guard) = trace.step(parent)
    {
        out.push((parent, guard.uses.clone()));
    }
    out.sort_by_key(|(step, _)| *step);
    out
}

fn label_offset(label: &str) -> Option<usize> {
    label.strip_prefix("in[")?.strip_suffix(']')?.parse().ok()
}

fn read_sets(program: &Program, run: &mut Run, accesses: &[Access]) -> Vec<u32> {
    let mut sets = Vec::new();
    for access in accesses {
        let (space, base, len) = match *access {
            Access::Reg { offset, size } => (program.register_space, offset, size),
            Access::Mem { addr, len } => (program.ram_space, addr, len),
        };
        for i in 0..u64::from(len) {
            if let Some(set) = run.emu.state.get_shadow_memory(space, base.wrapping_add(i)) {
                sets.push(set);
            }
        }
    }
    sets
}

/// Run (on either engine) to just before `step`, by breakpoint on that step's pc
/// and its occurrence count in the recorded trace.
fn run_jit_to(run: &mut Run, trace: &Trace, step: u64) -> Result<bool> {
    let Some(target) = trace.step(step) else {
        return Ok(false);
    };
    let pc = target.pc;
    let mut remaining = trace.steps[..step as usize]
        .iter()
        .filter(|s| s.pc == pc)
        .count()
        + 1;
    run.emu.set_breakpoint(pc);
    let reached = loop {
        if run.emu.inst_count == step && run.emu.pc == pc {
            break true;
        }
        match run.emu.resume() {
            Ok(fission_emulator::RunOutcome::HitBreakpoint(at)) if at == pc => {
                remaining -= 1;
                if remaining == 0 {
                    break run.emu.inst_count == step;
                }
            }
            _ => break false,
        }
    };
    run.emu.clear_breakpoint(pc);
    Ok(reached)
}

/// Run the incident again with every input byte as its own taint source and
/// report which sources reach the sink criterion.
///
/// Interpreter throughout: it retires exactly the requested number of
/// instructions, which the criterion steps need. (Since #1111 the two
/// engines' taint agree through memory, and a store through a tainted
/// pointer no longer panics the interpreter, so P1's JIT fallback is gone.)
pub fn dynamic_taint(program: &Program, trace: &Trace) -> Result<TaintResult> {
    dynamic_taint_on(program, trace, true)
}

fn dynamic_taint_on(program: &Program, trace: &Trace, interpreter: bool) -> Result<TaintResult> {
    let mut run = program.fresh(Engine::Interpreter)?;
    run.emu.set_shadow_mode(ShadowMode::Taint);
    let mut criteria = taint_criteria(trace);
    let mut result = TaintResult {
        engine: if interpreter { "interpreter" } else { "jit" }.into(),
        ..TaintResult::default()
    };
    let mut sets: Vec<u32> = Vec::new();
    let reads: Vec<(u64, Vec<(u64, usize)>)> = trace
        .steps
        .iter()
        .filter(|s| !s.input.is_empty())
        .map(|s| {
            (
                s.step + 1,
                s.input.iter().map(|b| (b.addr, b.offset)).collect(),
            )
        })
        .collect();
    let mut events: Vec<(u64, bool, usize)> = reads
        .iter()
        .enumerate()
        .map(|(i, (step, _))| (*step, false, i))
        .chain(
            criteria
                .iter()
                .enumerate()
                .map(|(i, (step, _))| (*step, true, i)),
        )
        .collect();
    events.sort();
    for (step, is_criterion, index) in events {
        if step > trace.steps.len() as u64 {
            continue;
        }
        if is_criterion {
            run.emu.force_interpreter = interpreter;
            // An exit criterion is read just before the last instruction
            // (the exit itself writes no memory).
            let at = step.min(trace.steps.len().saturating_sub(1) as u64);
            if !run_jit_to(&mut run, trace, at)? {
                break;
            }
        } else if !run.run_to_step(step)? {
            break;
        }
        if is_criterion {
            let accesses = std::mem::take(&mut criteria[index].1);
            result.criterion_bytes += accesses
                .iter()
                .map(|a| match a {
                    Access::Reg { size, .. } => *size as usize,
                    Access::Mem { len, .. } => *len as usize,
                })
                .sum::<usize>();
            sets.extend(read_sets(program, &mut run, &accesses));
        } else {
            // Overwrite the read's whole-buffer label with one per byte.
            for &(addr, offset) in &reads[index].1 {
                run.emu.taint_range(addr, 1, format!("in[{offset}]"));
            }
        }
    }
    for set in sets {
        let data = run.emu.taint.data_projection(set);
        if let Some(data) = data {
            result.data.extend(
                run.emu
                    .taint
                    .labels(data)
                    .iter()
                    .filter_map(|l| label_offset(l)),
            );
        }
        result.data_control.extend(
            run.emu
                .taint
                .labels(set)
                .iter()
                .filter_map(|l| label_offset(l)),
        );
    }
    result.control_complete = run.emu.taint.control_tracking_complete();
    Ok(result)
}
