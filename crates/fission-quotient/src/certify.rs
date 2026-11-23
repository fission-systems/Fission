//! White-box refutation of a candidate quotient.
//!
//! The slice's path summaries and the automaton are unrolled side by side
//! for `H` steps from the initial context, and the solver is asked for an
//! input word on which they differ -- in how a step ends, in a result, in a
//! call (its target or an observed argument), in a data slot, or by the
//! automaton having no transition. A step that ends in a call that does not
//! return halts the slice: every later step of the word is `Halted`. `Unsat` is a proof of bounded
//! observational equivalence *under the memory model*; `Sat` is a candidate
//! counterexample that the emulator must confirm before anything acts on it;
//! `Unknown` is neither and is never reported as a proof.

use std::collections::BTreeMap;

use fission_solver::{SatResult, Solver, SymExpr, SymNodeId};

use crate::boundary::QuotientBudget;
use crate::expr::{Bin, bin, ite, konst, mask, not, or_all, subst, width, zext};
use crate::oracle::{Exit, Symbol};
use crate::srqa::{CallTerm, SlotRoles, Srqa, path_exit};
use crate::summary::{SliceSummary, var_id};

pub enum Refutation {
    /// No word of length up to `H` tells them apart.
    Proved,
    /// A word the solver says distinguishes them, least first.
    Candidate(Vec<Symbol>),
    Unknown,
}

const STATE_BITS: u32 = 16;
const EXIT_BITS: u32 = 2;

fn exit_code(exit: Exit) -> SymExpr {
    konst(
        match exit {
            Exit::Returned => 0,
            Exit::NoReturn => 1,
            _ => 2,
        },
        EXIT_BITS,
    )
}

/// The observed calls of a step as fixed fields: for each of `count` calls,
/// whether it happens, its target, how many arguments it shows and each of
/// `arity` arguments (zero past the end).
fn call_fields(calls: &[CallTerm], count: usize, arity: usize) -> Vec<SymExpr> {
    let mut fields = Vec::with_capacity(count * (3 + arity));
    for n in 0..count {
        let call = calls.get(n);
        fields.push(konst(u64::from(call.is_some()), 1));
        fields.push(konst(call.map_or(0, |c| c.target), 64));
        fields.push(konst(call.map_or(0, |c| c.args.len() as u64), 8));
        for index in 0..arity {
            fields.push(
                call.and_then(|c| c.args.get(index))
                    .map_or_else(|| konst(0, 64), |arg| zext(arg.clone(), 64)),
            );
        }
    }
    fields
}

struct SubstitutedPath {
    guard: SymExpr,
    exit: SymExpr,
    output: SymExpr,
    post: Vec<SymExpr>,
    calls: Vec<SymExpr>,
}

/// Ask for a word of length `horizon` on which the slice and the automaton
/// differ: in a result or in the automaton having no transition, and -- with
/// `observe_data` -- in a data field.
#[must_use]
pub fn refute(
    summary: &SliceSummary,
    roles: &SlotRoles,
    srqa: &Srqa,
    initial_slots: &[u64],
    horizon: u32,
    budget: &QuotientBudget,
    observe_data: bool,
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
    // Whether an earlier step ended in a call that does not return.
    let mut halted = konst(0, 1);
    let path_calls = summary
        .paths
        .iter()
        .map(|path| {
            path.calls
                .iter()
                .map(|call| CallTerm {
                    target: call.target,
                    args: call.args.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let call_count = path_calls
        .iter()
        .map(Vec::len)
        .chain(srqa.transitions.iter().map(|t| t.calls.len()))
        .max()
        .unwrap_or(0);
    let arity = path_calls
        .iter()
        .flatten()
        .chain(srqa.transitions.iter().flat_map(|t| &t.calls))
        .map(|call| call.args.len())
        .max()
        .unwrap_or(0);

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
            .zip(&path_calls)
            .map(|(path, calls)| {
                let calls = calls
                    .iter()
                    .map(|call| CallTerm {
                        target: call.target,
                        args: call.args.iter().map(|arg| subst(arg, &map)).collect(),
                    })
                    .collect::<Vec<_>>();
                SubstitutedPath {
                    guard: subst(&path.guard, &map),
                    exit: exit_code(path_exit(path.exit)),
                    output: subst(&path.output, &map),
                    post: path
                        .post
                        .iter()
                        .map(|post| subst(post, &map))
                        .collect::<Vec<_>>(),
                    calls: call_fields(&calls, call_count, arity),
                }
            })
            .collect::<Vec<SubstitutedPath>>();
        // The paths are disjoint and cover every input, so the last one is
        // what remains when no earlier guard holds.
        let choose = |pick: &dyn Fn(&SubstitutedPath) -> SymExpr| {
            let (last, rest) = paths.split_last().expect("a slice has a path");
            rest.iter().rev().fold(pick(last), |otherwise, path| {
                ite(path.guard.clone(), pick(path), otherwise)
            })
        };
        // Once halted, a step shows only that, and changes nothing.
        let live_exit = choose(&|path| path.exit.clone());
        let exit = ite(halted.clone(), exit_code(Exit::Halted), live_exit.clone());
        let output = choose(&|path| path.output.clone());
        let output = ite(halted.clone(), konst(0, width(&output)), output);
        let calls = (0..call_count * (3 + arity))
            .map(|field| {
                let value = choose(&|path| path.calls[field].clone());
                ite(halted.clone(), konst(0, width(&value)), value)
            })
            .collect::<Vec<_>>();
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
                ite(
                    halted.clone(),
                    slots[slot].clone(),
                    choose(&|path| path.post[slot].clone()),
                ),
            ));
            next_slots.push(next);
        }
        let next_halted = bin(
            Bin::Or,
            halted.clone(),
            bin(Bin::Eq, live_exit, exit_code(Exit::NoReturn)),
        );

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
        let q_exit = pick(
            &|t| exit_code(srqa.transitions[t].exit),
            exit_code(Exit::Returned),
        );
        let q_call_fields = srqa
            .transitions
            .iter()
            .map(|t| {
                let calls = t
                    .calls
                    .iter()
                    .map(|call| CallTerm {
                        target: call.target,
                        args: call.args.iter().map(|arg| subst(arg, &qmap)).collect(),
                    })
                    .collect::<Vec<_>>();
                call_fields(&calls, call_count, arity)
            })
            .collect::<Vec<_>>();
        let q_calls = (0..call_count * (3 + arity))
            .map(|field| {
                let otherwise = konst(0, width(&calls[field]));
                pick(&|t| q_call_fields[t][field].clone(), otherwise)
            })
            .collect::<Vec<_>>();
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

        divergences.push(bin(Bin::Neq, exit, q_exit));
        divergences.push(bin(Bin::Neq, output, q_output));
        for (slice_field, quotient_field) in calls.into_iter().zip(q_calls) {
            divergences.push(bin(Bin::Neq, slice_field, quotient_field));
        }
        divergences.push(not(or_all(fires.clone())));
        if observe_data {
            for (index, slot) in roles.data.iter().enumerate() {
                divergences.push(bin(
                    Bin::Neq,
                    next_slots[*slot].clone(),
                    next_data[index].clone(),
                ));
            }
        }

        inputs.push(word_inputs);
        slots = next_slots;
        state = next_state;
        data = next_data;
        halted = next_halted;
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
