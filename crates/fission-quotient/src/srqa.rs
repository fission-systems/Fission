//! The symbolic register quotient automaton: learned control states, with
//! guards over the inputs and update expressions for the data slots taken
//! from the P-code path summaries.
//!
//! Context slots split into two roles. A *control* slot reaches a branch
//! condition or the result, directly or through another control slot; a
//! *data* slot reaches neither. Control slots are what the learner's states
//! stand for: each state carries the control valuation of the word that
//! reached it. Data slots never become states -- they are registers of the
//! automaton, updated symbolically -- which is what keeps an accumulator from
//! multiplying the state count.

use std::collections::{BTreeMap, BTreeSet};

use fission_solver::{SatResult, Solver, SymExpr, SymNodeId};

use crate::boundary::QuotientBudget;
use crate::expr::{self, Bin, as_const, bin, eval, free_vars, konst, not, or_all, subst};
use crate::learn::{Hypothesis, Learner};
use crate::oracle::{CallEvent, Exit, Observation, Symbol};
use crate::summary::{PathExit, SliceSummary, var_id};

/// Slot indices by role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRoles {
    pub control: Vec<usize>,
    pub data: Vec<usize>,
}

/// Split the context into control and data slots: the least set of slots
/// closed under "a guard, the result or a call argument reads it" and "a
/// control slot's next value reads it".
#[must_use]
pub fn factor(summary: &SliceSummary) -> SlotRoles {
    let slot_of = summary
        .slots
        .iter()
        .enumerate()
        .map(|(index, slot)| (slot.id(), index))
        .collect::<BTreeMap<_, _>>();
    let slots_in = |value: &SymExpr| {
        free_vars(value)
            .into_iter()
            .filter_map(|id| slot_of.get(&id).copied())
            .collect::<BTreeSet<_>>()
    };
    let mut control = BTreeSet::new();
    for path in &summary.paths {
        control.extend(slots_in(&path.guard));
        control.extend(slots_in(&path.output));
        for call in &path.calls {
            for arg in &call.args {
                control.extend(slots_in(arg));
            }
        }
    }
    loop {
        let before = control.len();
        for slot in control.clone() {
            for path in &summary.paths {
                control.extend(slots_in(&path.post[slot]));
            }
        }
        if control.len() == before {
            break;
        }
    }
    SlotRoles {
        control: control.iter().copied().collect(),
        data: (0..summary.slots.len())
            .filter(|slot| !control.contains(slot))
            .collect(),
    }
}

/// Read a slot's value out of context bytes.
#[must_use]
pub fn slot_value(context: &[u8], offset: u32, size: u32, big_endian: bool) -> u64 {
    let bytes = &context[offset as usize..(offset + size) as usize];
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate() {
        let shift = if big_endian {
            (size as usize - 1 - index) * 8
        } else {
            index * 8
        };
        value |= u64::from(*byte) << shift;
    }
    value
}

/// A call a transition makes: its target and observed arguments, over the
/// inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTerm {
    pub target: u64,
    pub args: Vec<SymExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotientTransition {
    pub from: usize,
    /// One-bit condition over the input variables.
    pub guard: SymExpr,
    /// The result, over the inputs and the data slots.
    pub output: SymExpr,
    /// The next value of each data slot, in `SlotRoles::data` order.
    pub updates: Vec<SymExpr>,
    /// The calls the step makes, in order.
    pub calls: Vec<CallTerm>,
    /// `Returned`, `NoReturn` (the step ends in a call that does not
    /// return), or `Halted` (the machine stopped at an earlier step).
    pub exit: Exit,
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Srqa {
    /// Per state, the value of each control slot (in `SlotRoles::control`
    /// order) after the state's access word.
    pub control_valuations: Vec<Vec<u64>>,
    pub transitions: Vec<QuotientTransition>,
}

/// One concrete step of the automaton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrqaStep {
    pub exit: Exit,
    pub output: u64,
    pub calls: Vec<CallEvent>,
    pub data: Vec<u64>,
}

impl SrqaStep {
    /// What the step shows the outside world.
    #[must_use]
    pub fn observation(&self) -> Observation {
        Observation {
            exit: self.exit,
            value: self.output,
            calls: self.calls.clone(),
        }
    }
}

/// Evaluate a transition's calls.
fn eval_calls(calls: &[CallTerm], env: &BTreeMap<SymNodeId, u64>) -> Option<Vec<CallEvent>> {
    calls
        .iter()
        .map(|call| {
            Some(CallEvent {
                target: call.target,
                args: call
                    .args
                    .iter()
                    .map(|arg| eval(arg, env))
                    .collect::<Option<_>>()?,
            })
        })
        .collect()
}

/// The exit a path summary's step ends with.
#[must_use]
pub fn path_exit(exit: PathExit) -> Exit {
    match exit {
        PathExit::Returned => Exit::Returned,
        PathExit::NoReturn => Exit::NoReturn,
    }
}

impl Srqa {
    /// Run `word` from state 0 with the data slots starting at `data`.
    /// `None` when no transition's guard holds -- the automaton is not total
    /// there, which is itself a disagreement.
    #[must_use]
    pub fn run(
        &self,
        summary: &SliceSummary,
        roles: &SlotRoles,
        mut data: Vec<u64>,
        word: &[Symbol],
    ) -> Option<Vec<SrqaStep>> {
        let mut state = 0;
        let mut out = Vec::with_capacity(word.len());
        for symbol in word {
            let mut env = BTreeMap::new();
            for (var, value) in summary.inputs.iter().zip(symbol) {
                env.insert(var_id(var), *value);
            }
            for (slot, value) in roles.data.iter().zip(&data) {
                env.insert(summary.slots[*slot].id(), *value);
            }
            let transition = self
                .transitions
                .iter()
                .filter(|t| t.from == state)
                .find(|t| eval(&t.guard, &env) == Some(1))?;
            let output = eval(&transition.output, &env)?;
            let calls = eval_calls(&transition.calls, &env)?;
            data = transition
                .updates
                .iter()
                .map(|update| eval(update, &env))
                .collect::<Option<_>>()?;
            out.push(SrqaStep {
                exit: transition.exit,
                output,
                calls,
                data: data.clone(),
            });
            state = transition.to;
        }
        Some(out)
    }
}

pub enum Built {
    Srqa(Srqa),
    /// No alphabet symbol exercises this part of a state's behaviour; add
    /// the symbol and learn again.
    NeedSymbol(Symbol),
    /// One path fans out to more control valuations than the split allows;
    /// these inputs reach some of them.
    FanOut(Vec<Symbol>),
    /// The summary and the emulator disagree, or a cell cannot be built.
    Unsupported(String),
}

/// How many distinct next control valuations one path may produce from one
/// state before the cell split gives up.
const MAX_CELL_SPLIT: usize = 16;

/// Build the automaton from a hypothesis whose states the learner reached
/// on the emulator.
pub fn build(
    summary: &SliceSummary,
    roles: &SlotRoles,
    hypothesis: &Hypothesis,
    learner: &Learner,
    big_endian: bool,
    budget: &QuotientBudget,
) -> Built {
    let input_ids = summary.inputs.iter().map(var_id).collect::<BTreeSet<_>>();
    let input_env = |symbol: &Symbol| -> BTreeMap<SymNodeId, u64> {
        summary
            .inputs
            .iter()
            .map(var_id)
            .zip(symbol.iter().copied())
            .collect()
    };
    let control_valuation = |node| -> Vec<u64> {
        roles
            .control
            .iter()
            .map(|slot| {
                let slot = &summary.slots[*slot];
                slot_value(learner.context(node), slot.offset, slot.size, big_endian)
            })
            .collect()
    };
    let control_valuations = hypothesis
        .states
        .iter()
        .map(|node| control_valuation(*node))
        .collect::<Vec<_>>();

    let mut transitions = Vec::new();
    for (state, valuation) in control_valuations.iter().enumerate() {
        // The state a call that does not return leads to: nothing runs from
        // it again. Its only transition is the halt itself.
        if hypothesis.transitions[state]
            .iter()
            .all(|(observation, target)| observation.exit == Exit::Halted && *target == state)
        {
            transitions.push(QuotientTransition {
                from: state,
                guard: konst(1, 1),
                output: konst(
                    0,
                    summary.paths.first().map_or(8, |p| expr::width(&p.output)),
                ),
                updates: roles
                    .data
                    .iter()
                    .map(|slot| summary.slots[*slot].var.clone())
                    .collect(),
                calls: Vec::new(),
                exit: Exit::Halted,
                to: state,
            });
            continue;
        }
        let fix = roles
            .control
            .iter()
            .zip(valuation)
            .map(|(slot, value)| {
                let slot = &summary.slots[*slot];
                (slot.id(), konst(*value, slot.size * 8))
            })
            .collect::<BTreeMap<SymNodeId, SymExpr>>();
        // (target, output, updates, calls, exit) -> guards, in first-seen
        // order.
        type Key = (usize, SymExpr, Vec<SymExpr>, Vec<CallTerm>, Exit);
        let mut groups: Vec<(Key, Vec<SymExpr>)> = Vec::new();
        for path in &summary.paths {
            let guard = subst(&path.guard, &fix);
            if as_const(&guard) == Some(0) {
                continue;
            }
            if !free_vars(&guard).is_subset(&input_ids) {
                return Built::Unsupported(
                    "a guard still reads the context once the control state is fixed".to_string(),
                );
            }
            let next = roles
                .control
                .iter()
                .map(|slot| subst(&path.post[*slot], &fix))
                .collect::<Vec<_>>();
            let output = subst(&path.output, &fix);
            let updates = roles
                .data
                .iter()
                .map(|slot| subst(&path.post[*slot], &fix))
                .collect::<Vec<_>>();
            let calls = path
                .calls
                .iter()
                .map(|call| CallTerm {
                    target: call.target,
                    args: call.args.iter().map(|arg| subst(arg, &fix)).collect(),
                })
                .collect::<Vec<_>>();
            let exit = path_exit(path.exit);
            // Where a step does not return, the control state it would leave
            // is never observed: nothing runs after it.
            let next = if exit == Exit::NoReturn {
                Vec::new()
            } else {
                next
            };
            let cells = match split_cells(&guard, &next, &summary.inputs, budget) {
                Ok(cells) => cells,
                Err(CellError::FanOut(symbols)) => return Built::FanOut(symbols),
                Err(CellError::Unsupported(reason)) => return Built::Unsupported(reason),
            };
            for (cell_guard, next_values, least) in cells {
                let symbol = learner
                    .alphabet
                    .iter()
                    .position(|symbol| eval(&cell_guard, &input_env(symbol)) == Some(1));
                let Some(symbol) = symbol else {
                    let least = match least {
                        Some(least) => Ok(Some(least)),
                        None => minimal_input(&cell_guard, &summary.inputs, budget),
                    };
                    return match least {
                        Ok(Some(symbol)) => Built::NeedSymbol(symbol),
                        Ok(None) => Built::Unsupported(
                            "a cell the guard folding kept has no input".to_string(),
                        ),
                        Err(()) => Built::Unsupported(
                            "the solver could not produce an input for a cell".to_string(),
                        ),
                    };
                };
                let (observation, target) = &hypothesis.transitions[state][symbol];
                let target = *target;
                // The summary and the machine must agree on the step the
                // representative takes, or nothing built from the summary
                // means anything.
                let mut env = input_env(&learner.alphabet[symbol]);
                for slot in &roles.data {
                    let slot = &summary.slots[*slot];
                    let value = slot_value(
                        learner.context(hypothesis.states[state]),
                        slot.offset,
                        slot.size,
                        big_endian,
                    );
                    env.insert(slot.id(), value);
                }
                let predicted = eval(&output, &env);
                let predicted_calls = eval_calls(&calls, &env);
                if observation.exit != exit
                    || predicted != Some(observation.value)
                    || predicted_calls.as_ref() != Some(&observation.calls)
                {
                    return Built::Unsupported(format!(
                        "the P-code summary predicts {exit:?} {predicted:?} with calls \
                         {predicted_calls:?} where the emulator observed {observation:?} \
                         (state {state}, input {:?})",
                        learner.alphabet[symbol]
                    ));
                }
                let child = learner
                    .access(hypothesis.states[state])
                    .into_iter()
                    .chain([symbol])
                    .collect::<Vec<_>>();
                if exit == Exit::Returned
                    && let Some(node) = learner.node_of(&child)
                    && control_valuation(node) != next_values
                {
                    return Built::Unsupported(format!(
                        "the P-code summary predicts control state {next_values:?} where the \
                         emulator left {:?} (state {state}, reached by {:?}, input {:?})",
                        control_valuation(node),
                        learner.symbols(&learner.access(hypothesis.states[state])),
                        learner.alphabet[symbol]
                    ));
                }
                let key = (target, output.clone(), updates.clone(), calls.clone(), exit);
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, guards)) => guards.push(cell_guard),
                    None => groups.push((key, vec![cell_guard])),
                }
            }
        }
        for ((to, output, updates, calls, exit), guards) in groups {
            transitions.push(QuotientTransition {
                from: state,
                guard: or_all(guards),
                output,
                updates,
                calls,
                exit,
                to,
            });
        }
    }
    Built::Srqa(Srqa {
        control_valuations,
        transitions,
    })
}

/// Why a path could not be split into cells.
enum CellError {
    /// More next control valuations than [`MAX_CELL_SPLIT`]; these are the
    /// least inputs reaching the first of them.
    FanOut(Vec<Symbol>),
    Unsupported(String),
}

/// A guard, the control valuation it leads to, and the least input in it
/// (when the split had to find one).
type Cell = (SymExpr, Vec<u64>, Option<Symbol>);

/// Split one path's guard by the control valuation it leads to, each cell
/// with the least input that reaches it. Cells are found in order of that
/// input, so the split does not depend on which models the solver happens
/// to produce.
fn split_cells(
    guard: &SymExpr,
    next: &[SymExpr],
    inputs: &[SymExpr],
    budget: &QuotientBudget,
) -> Result<Vec<Cell>, CellError> {
    if let Some(values) = next.iter().map(as_const).collect::<Option<Vec<_>>>() {
        return Ok(vec![(guard.clone(), values, None)]);
    }
    let mut cells: Vec<Cell> = Vec::new();
    let mut remaining = guard.clone();
    loop {
        let symbol = match minimal_input(&remaining, inputs, budget) {
            Ok(Some(symbol)) => symbol,
            Ok(None) => break,
            Err(()) => {
                return Err(CellError::Unsupported(
                    "the solver could not enumerate a path's next control states".to_string(),
                ));
            }
        };
        let env = inputs
            .iter()
            .map(var_id)
            .zip(symbol.iter().copied())
            .collect();
        let Some(values) = next
            .iter()
            .map(|value| eval(value, &env))
            .collect::<Option<Vec<_>>>()
        else {
            return Err(CellError::Unsupported(
                "a next control state reads more than the inputs".to_string(),
            ));
        };
        let same = expr::and_all(
            next.iter()
                .zip(&values)
                .map(|(value, v)| bin(Bin::Eq, value.clone(), konst(*v, expr::width(value)))),
        );
        cells.push((
            bin(Bin::And, guard.clone(), same.clone()),
            values,
            Some(symbol),
        ));
        if cells.len() > MAX_CELL_SPLIT {
            return Err(CellError::FanOut(
                cells.into_iter().filter_map(|cell| cell.2).collect(),
            ));
        }
        remaining = bin(Bin::And, remaining, not(same));
    }
    cells.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(cells)
}

/// The least input (lexicographically, port by port) satisfying
/// `constraint`: `Ok(None)` when there is none, `Err` when the solver cannot
/// tell. Deterministic: each port is the minimum the solver proves, whatever
/// model it happened to find first.
#[allow(clippy::result_unit_err)]
pub fn minimal_input(
    constraint: &SymExpr,
    inputs: &[SymExpr],
    budget: &QuotientBudget,
) -> Result<Option<Symbol>, ()> {
    let mut solver = Solver::new();
    solver.set_conflict_budget(budget.conflict_budget);
    for input in inputs {
        solver.nodes.insert(var_id(input), input.clone());
    }
    solver.assert(constraint.clone());
    match solver.check_sat().unwrap_or(SatResult::Unknown) {
        SatResult::Unsat => return Ok(None),
        SatResult::Unknown => return Err(()),
        SatResult::Sat => {}
    }
    let mut symbol = Vec::with_capacity(inputs.len());
    for input in inputs {
        let value = solver.min(input).ok_or(())?;
        solver.assert(SymExpr::new_eq(
            input.clone(),
            konst(value, expr::width(input)),
        ));
        symbol.push(value);
    }
    Ok(Some(symbol))
}
