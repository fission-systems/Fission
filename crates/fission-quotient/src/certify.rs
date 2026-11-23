//! White-box refutation of a candidate quotient.
//!
//! The slice's path summaries and the automaton are unrolled side by side
//! for `H` steps from the initial context, and the solver is asked for an
//! input word on which they differ -- in a result, in a data slot, or by the
//! automaton having no transition. `Unsat` is a proof of bounded
//! observational equivalence *under the memory model*; `Sat` is a candidate
//! counterexample that the emulator must confirm before anything acts on it;
//! `Unknown` is neither and is never reported as a proof.

use std::collections::BTreeMap;

use fission_solver::{SatResult, Solver, SymExpr, SymNodeId};

use crate::boundary::QuotientBudget;
use crate::expr::{Bin, bin, ite, konst, mask, not, or_all, subst, width};
use crate::oracle::Symbol;
use crate::srqa::{SlotRoles, Srqa};
use crate::summary::{SliceSummary, var_id};

pub enum Refutation {
    /// No word of length up to `H` tells them apart.
    Proved,
    /// A word the solver says distinguishes them, least first.
    Candidate(Vec<Symbol>),
    Unknown,
}

const STATE_BITS: u32 = 16;

type SubstitutedPath = (SymExpr, SymExpr, Vec<SymExpr>);

/// Ask for a word of length `horizon` on which the slice and the automaton
/// differ.
#[must_use]
pub fn refute(
    summary: &SliceSummary,
    roles: &SlotRoles,
    srqa: &Srqa,
    initial_slots: &[u64],
    horizon: u32,
    budget: &QuotientBudget,
) -> Refutation {
    let mut solver = Solver::new();
    solver.set_conflict_budget(budget.conflict_budget);
    let fresh = |solver: &mut Solver, name: String, bits: u32| {
        let var = SymExpr::new_var(&name, bits);
        solver.nodes.insert(var_id(&var), var.clone());
        var
    };

    let slot_bits = |slot: usize| summary.slots[slot].size * 8;
    let mut slots: Vec<SymExpr> = initial_slots
        .iter()
        .enumerate()
        .map(|(slot, value)| konst(*value, slot_bits(slot)))
        .collect();
    let mut state = konst(0, STATE_BITS);
    let mut data: Vec<SymExpr> = roles.data.iter().map(|slot| slots[*slot].clone()).collect();
    let mut inputs = Vec::new();
    let mut divergences = Vec::new();

    for step in 0..horizon {
        let word_inputs = summary
            .inputs
            .iter()
            .enumerate()
            .map(|(port, var)| fresh(&mut solver, format!("x{step}_{port}"), width(var)))
            .collect::<Vec<_>>();

        // The slice.
        let mut map: BTreeMap<SymNodeId, SymExpr> = summary
            .inputs
            .iter()
            .map(var_id)
            .zip(word_inputs.iter().cloned())
            .collect();
        for (slot, value) in summary.slots.iter().zip(&slots) {
            map.insert(slot.id(), value.clone());
        }
        let paths = summary
            .paths
            .iter()
            .map(|path| {
                (
                    subst(&path.guard, &map),
                    subst(&path.output, &map),
                    path.post
                        .iter()
                        .map(|post| subst(post, &map))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<SubstitutedPath>>();
        // The paths are disjoint and cover every input, so the last one is
        // what remains when no earlier guard holds.
        let choose = |pick: &dyn Fn(&SubstitutedPath) -> SymExpr| {
            let (last, rest) = paths.split_last().expect("a slice has a path");
            rest.iter().rev().fold(pick(last), |otherwise, path| {
                ite(path.0.clone(), pick(path), otherwise)
            })
        };
        let output = choose(&|path| path.1.clone());
        let mut next_slots = Vec::with_capacity(slots.len());
        for slot in 0..slots.len() {
            let next = fresh(
                &mut solver,
                format!("s{}_{slot}", step + 1),
                slot_bits(slot),
            );
            solver.assert(bin(
                Bin::Eq,
                next.clone(),
                choose(&|path| path.2[slot].clone()),
            ));
            next_slots.push(next);
        }

        // The automaton.
        let mut qmap: BTreeMap<SymNodeId, SymExpr> = summary
            .inputs
            .iter()
            .map(var_id)
            .zip(word_inputs.iter().cloned())
            .collect();
        for (slot, value) in roles.data.iter().zip(&data) {
            qmap.insert(summary.slots[*slot].id(), value.clone());
        }
        let fires = srqa
            .transitions
            .iter()
            .map(|t| {
                bin(
                    Bin::And,
                    bin(Bin::Eq, state.clone(), konst(t.from as u64, STATE_BITS)),
                    subst(&t.guard, &qmap),
                )
            })
            .collect::<Vec<_>>();
        let pick = |values: &dyn Fn(usize) -> SymExpr, otherwise: SymExpr| {
            fires
                .iter()
                .enumerate()
                .rev()
                .fold(otherwise, |rest, (index, fire)| {
                    ite(fire.clone(), values(index), rest)
                })
        };
        let output_bits = width(&output);
        let q_output = pick(
            &|t| subst(&srqa.transitions[t].output, &qmap),
            konst(0, output_bits),
        );
        let next_state = fresh(&mut solver, format!("q{}", step + 1), STATE_BITS);
        solver.assert(bin(
            Bin::Eq,
            next_state.clone(),
            pick(
                &|t| konst(srqa.transitions[t].to as u64, STATE_BITS),
                konst(mask(STATE_BITS), STATE_BITS),
            ),
        ));
        let mut next_data = Vec::with_capacity(data.len());
        for (index, slot) in roles.data.iter().enumerate() {
            let next = fresh(
                &mut solver,
                format!("d{}_{slot}", step + 1),
                slot_bits(*slot),
            );
            solver.assert(bin(
                Bin::Eq,
                next.clone(),
                pick(
                    &|t| subst(&srqa.transitions[t].updates[index], &qmap),
                    data[index].clone(),
                ),
            ));
            next_data.push(next);
        }

        divergences.push(bin(Bin::Neq, output, q_output));
        divergences.push(not(or_all(fires.clone())));
        for (index, slot) in roles.data.iter().enumerate() {
            divergences.push(bin(
                Bin::Neq,
                next_slots[*slot].clone(),
                next_data[index].clone(),
            ));
        }

        inputs.push(word_inputs);
        slots = next_slots;
        state = next_state;
        data = next_data;
    }

    solver.assert(or_all(divergences));
    match solver.check_sat().unwrap_or(SatResult::Unknown) {
        SatResult::Unsat => Refutation::Proved,
        SatResult::Unknown => Refutation::Unknown,
        SatResult::Sat => {
            // The model as found, before minimizing moves it.
            let found = inputs
                .iter()
                .map(|step| {
                    step.iter()
                        .map(|input| solver.bv_theory.evaluate_expr_in_model(&solver.sat, input))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            // The least word, so a rerun finds the same one. If the solver
            // cannot finish minimizing, the model it found is still a
            // distinguishing word.
            let mut word = Vec::with_capacity(inputs.len());
            for step_inputs in &inputs {
                let mut symbol = Vec::with_capacity(step_inputs.len());
                for input in step_inputs {
                    let Some(value) = solver.min(input) else {
                        return Refutation::Candidate(found);
                    };
                    solver.assert(bin(Bin::Eq, input.clone(), konst(value, width(input))));
                    symbol.push(value);
                }
                word.push(symbol);
            }
            Refutation::Candidate(word)
        }
    }
}

/// Deterministic probe words for when the solver cannot decide: alphabet
/// symbols interleaved with boundary values of each port.
#[must_use]
pub fn probe_words(
    alphabet: &[Symbol],
    port_bits: &[u32],
    count: u32,
    horizon: u32,
) -> Vec<Vec<Symbol>> {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    (0..count)
        .map(|_| {
            (0..horizon.max(1))
                .map(|_| {
                    let roll = next();
                    if roll % 4 != 0 && !alphabet.is_empty() {
                        alphabet[(next() % alphabet.len() as u64) as usize].clone()
                    } else {
                        port_bits
                            .iter()
                            .map(|bits| {
                                let m = mask(*bits);
                                match next() % 4 {
                                    0 => 0,
                                    1 => m,
                                    2 => 1,
                                    _ => next() & m,
                                }
                            })
                            .collect()
                    }
                })
                .collect()
        })
        .collect()
}
