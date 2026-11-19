//! Which variables a read can observe holding the value they had on entry.
//!
//! A flow-insensitive fact about a variable -- "it only ever holds what these
//! assignments store" -- is true only if no read can see the value the
//! variable had before the body ran. That value is unknown: a global the
//! body reads but never writes, a register the caller left behind, a stack
//! slot written through a pointer. Ghidra's non-zero mask gives a function
//! input every bit for the same reason.
//!
//! The analysis is a forward "definitely assigned" pass over structured
//! PreHIR. A read of a name that is not definitely assigned on every path
//! from entry to it can see the entry value. A name whose address is taken
//! can be written where no assignment in the body shows it, so it is
//! reported as well.
//!
//! `goto` makes a label a join of every jump to it, including jumps that
//! come later in the text, so labels are solved to a fixed point over whole
//! passes. Loops are solved by re-walking the body until its back edge adds
//! nothing. Both start optimistic and only shrink, which is the standard
//! greatest fixed point of a must-analysis.

use crate::analysis::defuse::collect_expr_vars;
use crate::{HashMap, HashSet};
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

/// Passes over the body before giving up on a fixed point for labels.
const MAX_LABEL_ROUNDS: usize = 16;

/// Names that some read in `body` can observe holding their value from
/// before the body ran, plus every name whose address is taken.
pub fn names_reading_entry_values(body: &[PreHirStmt]) -> HashSet<String> {
    let mut label_in: HashMap<String, State> = HashMap::default();
    for _ in 0..MAX_LABEL_ROUNDS {
        let mut walk = Walk::new(&label_in, false);
        walk.stmts(body, State::Defined(HashSet::default()));
        if walk.goto_out == label_in {
            return walk.exposed;
        }
        label_in = walk.goto_out;
    }
    // No fixed point within the budget: every name read anywhere.
    let mut walk = Walk::new(&label_in, true);
    walk.stmts(body, State::Defined(HashSet::default()));
    walk.exposed
}

/// The names definitely assigned on every path reaching a point, or
/// `Unreachable` when no path reaches it yet. `Unreachable` is the identity
/// of `meet`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Unreachable,
    Defined(HashSet<String>),
}

impl State {
    fn meet(self, other: &State) -> State {
        match (self, other) {
            (State::Unreachable, other) => other.clone(),
            (this, State::Unreachable) => this,
            (State::Defined(mut a), State::Defined(b)) => {
                a.retain(|name| b.contains(name));
                State::Defined(a)
            }
        }
    }
}

#[derive(Default)]
struct Exits {
    breaks: Option<State>,
    continues: Option<State>,
    is_loop: bool,
}

struct Walk<'a> {
    /// Each label's meet over every jump to it, from the previous pass.
    label_in: &'a HashMap<String, State>,
    /// Each label's meet over the jumps seen in this pass.
    goto_out: HashMap<String, State>,
    exits: Vec<Exits>,
    exposed: HashSet<String>,
    /// Report every name read, reachable or not.
    pessimistic: bool,
}

fn meet_into(slot: &mut Option<State>, state: &State) {
    *slot = Some(match slot.take() {
        Some(old) => old.meet(state),
        None => state.clone(),
    });
}

impl<'a> Walk<'a> {
    fn new(label_in: &'a HashMap<String, State>, pessimistic: bool) -> Self {
        Self {
            label_in,
            goto_out: HashMap::default(),
            exits: Vec::new(),
            exposed: HashSet::default(),
            pessimistic,
        }
    }

    fn read(&mut self, expr: &PreHirExpr, state: &State) {
        collect_address_taken(expr, &mut self.exposed);
        let defined = match state {
            State::Defined(defined) if !self.pessimistic => defined,
            State::Unreachable if !self.pessimistic => return,
            _ => {
                collect_expr_vars(expr, &mut self.exposed);
                return;
            }
        };
        let mut names = HashSet::default();
        collect_expr_vars(expr, &mut names);
        self.exposed
            .extend(names.into_iter().filter(|name| !defined.contains(name)));
    }

    fn read_lvalue(&mut self, lhs: &PreHirLValue, state: &State) {
        match lhs {
            PreHirLValue::Var(_) => {}
            PreHirLValue::Deref { ptr, .. } => self.read(ptr, state),
            PreHirLValue::Index { base, index, .. } => {
                self.read(base, state);
                self.read(index, state);
            }
            PreHirLValue::FieldAccess { base, .. } => self.read(base, state),
        }
    }

    fn stmts(&mut self, stmts: &[PreHirStmt], mut state: State) -> State {
        for stmt in stmts {
            state = self.stmt(stmt, state);
        }
        state
    }

    fn jump(&mut self, label: &str, state: &State) {
        let slot = self
            .goto_out
            .entry(label.to_string())
            .or_insert(State::Unreachable);
        *slot = std::mem::replace(slot, State::Unreachable).meet(state);
    }

    fn stmt(&mut self, stmt: &PreHirStmt, state: State) -> State {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                self.read(rhs, &state);
                self.read_lvalue(lhs, &state);
                match (lhs, state) {
                    (PreHirLValue::Var(name), State::Defined(mut defined)) => {
                        defined.insert(name.clone());
                        State::Defined(defined)
                    }
                    (_, state) => state,
                }
            }
            PreHirStmt::Expr(expr) | PreHirStmt::VaStart { va_list: expr, .. } => {
                self.read(expr, &state);
                state
            }
            PreHirStmt::Block(body) => self.stmts(body, state),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.read(cond, &state);
                let then_out = self.stmts(then_body, state.clone());
                let else_out = self.stmts(else_body, state);
                then_out.meet(&else_out)
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.read(expr, &state);
                self.exits.push(Exits::default());
                // An arm is entered from the switch or by falling out of the
                // arm before it.
                let mut fall = State::Unreachable;
                for body in cases.iter().map(|case| &case.body).chain([default]) {
                    let entry = state.clone().meet(&fall);
                    fall = self.stmts(body, entry);
                }
                let exits = self.exits.pop().unwrap_or_default();
                let mut out = fall.meet(&state);
                if let Some(breaks) = exits.breaks {
                    out = out.meet(&breaks);
                }
                out
            }
            PreHirStmt::While { cond, body } => self.walk_loop(state, Some(cond), None, body, true),
            PreHirStmt::DoWhile { body, cond } => {
                self.walk_loop(state, Some(cond), None, body, false)
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let state = match init {
                    Some(init) => self.stmt(init, state),
                    None => state,
                };
                self.walk_loop(state, cond.as_ref(), update.as_deref(), body, true)
            }
            PreHirStmt::Label(label) => {
                let mut state = state;
                if let Some(earlier) = self.label_in.get(label) {
                    state = state.meet(earlier);
                }
                if let Some(seen) = self.goto_out.get(label) {
                    state = state.meet(seen);
                }
                state
            }
            PreHirStmt::Goto(label) => {
                self.jump(label, &state);
                State::Unreachable
            }
            PreHirStmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.read(expr, &state);
                }
                State::Unreachable
            }
            PreHirStmt::Break => {
                if let Some(exits) = self.exits.last_mut() {
                    meet_into(&mut exits.breaks, &state);
                }
                State::Unreachable
            }
            PreHirStmt::Continue => {
                if let Some(exits) = self.exits.iter_mut().rev().find(|exits| exits.is_loop) {
                    meet_into(&mut exits.continues, &state);
                }
                State::Unreachable
            }
        }
    }

    /// `test_first`: the condition is checked before the first iteration
    /// (`while`, `for`) rather than after it (`do`/`while`).
    fn walk_loop(
        &mut self,
        before: State,
        cond: Option<&PreHirExpr>,
        update: Option<&PreHirStmt>,
        body: &[PreHirStmt],
        test_first: bool,
    ) -> State {
        let mut entry = before;
        loop {
            if test_first && let Some(cond) = cond {
                self.read(cond, &entry);
            }
            self.exits.push(Exits {
                is_loop: true,
                ..Exits::default()
            });
            let end = self.stmts(body, entry.clone());
            let exits = self.exits.pop().unwrap_or_default();
            let mut back = match &exits.continues {
                Some(continues) => end.meet(continues),
                None => end,
            };
            if let Some(update) = update {
                back = self.stmt(update, back);
            }
            if !test_first && let Some(cond) = cond {
                self.read(cond, &back);
            }
            let next = entry.clone().meet(&back);
            if next == entry {
                // The loop leaves when its condition fails (at entry or at
                // the back edge) or through a `break`. With no condition it
                // leaves only through a `break`.
                let mut out = match cond {
                    Some(_) if test_first => entry.meet(&back),
                    Some(_) => back,
                    None => State::Unreachable,
                };
                if let Some(breaks) = exits.breaks {
                    out = out.meet(&breaks);
                }
                return out;
            }
            entry = next;
        }
    }
}

/// Every local whose address is taken: a store through that address, or a
/// callee given it, changes the local where no assignment shows it.
fn collect_address_taken(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => collect_address_taken(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_address_taken(lhs, out);
            collect_address_taken(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_address_taken(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_address_taken(base, out);
            collect_address_taken(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_address_taken(cond, out);
            collect_address_taken(then_expr, out);
            collect_address_taken(else_expr, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::NirType;
    use std::rc::Rc;

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn one() -> PreHirExpr {
        PreHirExpr::Const(
            1,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn use_of(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(var(name))
    }

    fn exposed(body: &[PreHirStmt]) -> Vec<String> {
        let mut names: Vec<_> = names_reading_entry_values(body).into_iter().collect();
        names.sort();
        names
    }

    /// `x = g; use(x)`: `g` is read but never written, so the read sees
    /// whatever it held on entry; `x` is written first.
    #[test]
    fn a_name_read_before_any_write_sees_its_entry_value() {
        assert_eq!(exposed(&[assign("x", var("g")), use_of("x")]), ["g"]);
    }

    /// `if (c) { x = 1; } use(x)`: the else path reaches the read with `x`
    /// unwritten.
    #[test]
    fn a_write_on_one_arm_does_not_cover_the_join() {
        let body = [
            PreHirStmt::If {
                cond: var("c"),
                then_body: Rc::new(vec![assign("x", one())]),
                else_body: Rc::new(vec![]),
            },
            use_of("x"),
        ];
        assert_eq!(exposed(&body), ["c", "x"]);
    }

    /// `goto M; x = 1; L: use(x); M: goto L;` -- `L` is first reached by the
    /// backward jump, with `x` never written. Only a fixed point over the
    /// later jump sees it.
    #[test]
    fn a_backward_jump_joins_the_label_it_targets() {
        let body = [
            PreHirStmt::Goto("M".into()),
            assign("x", one()),
            PreHirStmt::Label("L".into()),
            use_of("x"),
            PreHirStmt::Label("M".into()),
            PreHirStmt::Goto("L".into()),
        ];
        assert_eq!(exposed(&body), ["x"]);
        // Control: without the jump over the write, every path to the read
        // passes it.
        let covered = [
            assign("x", one()),
            PreHirStmt::Label("L".into()),
            use_of("x"),
            PreHirStmt::Goto("L".into()),
        ];
        assert!(exposed(&covered).is_empty());
    }

    /// `while (c) { use(x); x = 1; }`: the first iteration reads `x` before
    /// the body writes it.
    #[test]
    fn a_loop_body_read_before_its_write_sees_the_entry_value() {
        let body = [PreHirStmt::While {
            cond: var("c"),
            body: Rc::new(vec![use_of("x"), assign("x", one())]),
        }];
        assert_eq!(exposed(&body), ["c", "x"]);
    }

    /// `x = 1; p = &x; use(x)`: the local's address escapes, so it can change
    /// where no assignment shows it.
    #[test]
    fn an_address_taken_local_is_reported() {
        let body = [
            assign("x", one()),
            assign("p", PreHirExpr::AddressOfLocal("x".into())),
            use_of("x"),
        ];
        assert_eq!(exposed(&body), ["x"]);
    }
}
