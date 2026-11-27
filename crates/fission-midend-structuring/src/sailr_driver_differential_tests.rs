//! Differential checks for the SAILR driver: the structured body it returns
//! must do what the CFG it was given does.
//!
//! Each fixture is a CFG whose blocks carry statements and a terminator, run
//! through [`fold_sailr`] on a [`MockRotatedLoopHost`]. Two small
//! interpreters then execute the same initial states: one walks the CFG block
//! by block, the other runs the structured statements (with C `goto`
//! semantics, entering a loop body through a label without testing the
//! condition). What is compared is everything a caller could observe: the
//! returned value, the final memory and the ordered list of calls. Fuel is
//! charged per executed assignment/expression statement only, which both
//! sides perform identically when the transformation is sound, so a
//! non-terminating path runs out of fuel at the same point on both sides.

use super::*;
use crate::linear_types::LoweredTerminator;
use crate::loops::tests::MockRotatedLoopHost;
use fission_midend_core::ir::NirType;
use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt, PreHirUnaryOp};
use std::collections::BTreeMap;

const BASE: u64 = 0x1000;
const MEM_CELLS: i64 = 8;
const FUEL: u32 = 400;

fn addr(block: usize) -> u64 {
    BASE + 0x10 * block as u64
}

// ---------------------------------------------------------------- fixtures --

#[derive(Clone, Debug)]
enum Term {
    Goto(usize),
    Cond(PreHirExpr, usize, usize),
    Ret(Option<PreHirExpr>),
}

#[derive(Clone, Debug)]
struct Program {
    stmts: Vec<Vec<PreHirStmt>>,
    terms: Vec<Term>,
    /// Return joins, as the builder reports them: `(pred, join) -> value`.
    /// Entering `join` from `pred` returns `value` as it stood at the end of
    /// `pred`; the join's own statements are epilogue (register restores and
    /// return-register moves) and its own `return` expression is only right
    /// for some predecessors -- the builder's generic lowering of a merged
    /// return, which is why `lower_return_join_expr_for_predecessor` exists.
    joins: Vec<((usize, usize), PreHirExpr)>,
}

impl Program {
    fn successors(&self) -> Vec<Vec<usize>> {
        self.terms
            .iter()
            .map(|t| match t {
                Term::Goto(j) => vec![*j],
                Term::Cond(_, a, b) if a == b => vec![*a],
                Term::Cond(_, a, b) => vec![*a, *b],
                Term::Ret(_) => Vec::new(),
            })
            .collect()
    }

    fn host(&self) -> MockRotatedLoopHost {
        let terminators = self
            .terms
            .iter()
            .map(|t| match t {
                Term::Goto(j) => LoweredTerminator::Goto(addr(*j)),
                Term::Cond(c, a, b) => LoweredTerminator::Cond {
                    cond: c.clone(),
                    true_target: addr(*a),
                    false_target: Some(addr(*b)),
                },
                Term::Ret(e) => LoweredTerminator::Return(e.clone()),
            })
            .collect();
        let mut host = MockRotatedLoopHost::new(
            self.successors(),
            (0..self.terms.len()).map(addr).collect(),
            self.stmts.clone(),
            terminators,
        );
        host.return_join_exprs = self.joins.clone();
        host
    }

    fn all_reachable(&self) -> bool {
        self.reachable().iter().all(|&r| r)
    }

    fn reachable(&self) -> Vec<bool> {
        let succ = self.successors();
        let mut seen = vec![false; succ.len()];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(n) = stack.pop() {
            for &s in &succ[n] {
                if !seen[s] {
                    seen[s] = true;
                    stack.push(s);
                }
            }
        }
        seen
    }

    fn reachable_exits(&self) -> usize {
        let succ = self.successors();
        let mut seen = vec![false; succ.len()];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(n) = stack.pop() {
            for &s in &succ[n] {
                if !seen[s] {
                    seen[s] = true;
                    stack.push(s);
                }
            }
        }
        (0..succ.len())
            .filter(|&b| seen[b] && succ[b].is_empty())
            .count()
    }
}

fn v(name: &str) -> PreHirExpr {
    PreHirExpr::Var(name.to_string())
}
fn c(value: i64) -> PreHirExpr {
    PreHirExpr::Const(
        value,
        NirType::Int {
            bits: 32,
            signed: true,
        },
    )
}
fn bin(op: PreHirBinaryOp, lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    }
}
fn load(ptr: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Load {
        ptr: Box::new(ptr),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    }
}
fn call(target: &str, args: Vec<PreHirExpr>) -> PreHirExpr {
    PreHirExpr::Call {
        origin: None,
        target: target.to_string(),
        args,
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    }
}
fn set(name: &str, rhs: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name.to_string()),
        rhs,
    }
}
fn store(cell: PreHirExpr, rhs: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(cell),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        },
        rhs,
    }
}
fn tick() -> PreHirStmt {
    set("n", bin(PreHirBinaryOp::Add, v("n"), c(1)))
}

// ------------------------------------------------------------ interpreter --

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    vars: BTreeMap<String, i64>,
    mem: BTreeMap<i64, i64>,
    calls: Vec<(String, Vec<i64>)>,
    fuel: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Returned(Option<i64>),
    OutOfFuel,
}

/// What a caller can observe of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    outcome: Outcome,
    mem: BTreeMap<i64, i64>,
    calls: Vec<(String, Vec<i64>)>,
}

fn cell(ptr: i64) -> i64 {
    ptr.rem_euclid(MEM_CELLS)
}

fn eval(e: &PreHirExpr, st: &mut State) -> i64 {
    use PreHirBinaryOp as B;
    match e {
        PreHirExpr::Var(n) => st.vars.get(n).copied().unwrap_or(0),
        PreHirExpr::Const(x, _) => *x,
        PreHirExpr::Cast { expr, .. } => eval(expr, st),
        PreHirExpr::Unary { op, expr, .. } => {
            let x = eval(expr, st);
            match op {
                PreHirUnaryOp::Not => i64::from(x == 0),
                PreHirUnaryOp::BitNot => !x,
                PreHirUnaryOp::Neg => x.wrapping_neg(),
            }
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(op, B::LogicalAnd) {
                return i64::from(eval(lhs, st) != 0 && eval(rhs, st) != 0);
            }
            if matches!(op, B::LogicalOr) {
                return i64::from(eval(lhs, st) != 0 || eval(rhs, st) != 0);
            }
            let (a, b) = (eval(lhs, st), eval(rhs, st));
            match op {
                B::Add => a.wrapping_add(b),
                B::Sub => a.wrapping_sub(b),
                B::Mul => a.wrapping_mul(b),
                B::Div => a.checked_div(b).unwrap_or(0),
                B::Mod => a.checked_rem(b).unwrap_or(0),
                B::And => a & b,
                B::Or => a | b,
                B::Xor => a ^ b,
                B::Shl => a.wrapping_shl((b & 63) as u32),
                B::Shr => ((a as u64) >> (b & 63)) as i64,
                B::Sar => a >> (b & 63),
                B::Eq => i64::from(a == b),
                B::Ne => i64::from(a != b),
                B::Lt => i64::from((a as u64) < (b as u64)),
                B::Le => i64::from((a as u64) <= (b as u64)),
                B::Gt => i64::from((a as u64) > (b as u64)),
                B::Ge => i64::from((a as u64) >= (b as u64)),
                B::SLt => i64::from(a < b),
                B::SLe => i64::from(a <= b),
                B::SGt => i64::from(a > b),
                B::SGe => i64::from(a >= b),
                B::LogicalAnd | B::LogicalOr => unreachable!(),
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            if eval(cond, st) != 0 {
                eval(then_expr, st)
            } else {
                eval(else_expr, st)
            }
        }
        PreHirExpr::Load { ptr, .. } => {
            let p = cell(eval(ptr, st));
            st.mem.get(&p).copied().unwrap_or(0)
        }
        PreHirExpr::Call { target, args, .. } => {
            let args: Vec<i64> = args.iter().map(|a| eval(a, st)).collect();
            let result = args.iter().fold(target.len() as i64, |h, a| {
                h.wrapping_mul(31).wrapping_add(*a)
            });
            // A call may write memory: it is the effect duplication must not
            // repeat or lose.
            let slot = cell(result);
            *st.mem.entry(slot).or_default() += 1;
            st.calls.push((target.clone(), args));
            result & 0xff
        }
        other => panic!("fixture interpreter: unsupported expression {other:?}"),
    }
}

/// Charge one unit of fuel for an executed effect statement.
fn burn(st: &mut State) -> bool {
    if st.fuel == 0 {
        return false;
    }
    st.fuel -= 1;
    true
}

fn exec_effect(s: &PreHirStmt, st: &mut State) {
    match s {
        PreHirStmt::Assign { lhs, rhs } => {
            let value = eval(rhs, st);
            match lhs {
                PreHirLValue::Var(n) => {
                    st.vars.insert(n.clone(), value);
                }
                PreHirLValue::Deref { ptr, .. } => {
                    let p = cell(eval(ptr, st));
                    st.mem.insert(p, value);
                }
                other => panic!("fixture interpreter: unsupported lvalue {other:?}"),
            }
        }
        PreHirStmt::Expr(e) => {
            eval(e, st);
        }
        other => panic!("not an effect statement: {other:?}"),
    }
}

/// Run the CFG itself: the reference semantics.
fn run_cfg(p: &Program, mut st: State) -> Observation {
    let mut b = 0usize;
    let mut came_from: Option<usize> = None;
    let outcome = 'run: loop {
        if let Some(pred) = came_from
            && let Some((_, value)) = p.joins.iter().find(|(edge, _)| *edge == (pred, b))
        {
            let value = eval(value, &mut st);
            break Outcome::Returned(Some(value));
        }
        for s in &p.stmts[b] {
            if !burn(&mut st) {
                break 'run Outcome::OutOfFuel;
            }
            exec_effect(s, &mut st);
        }
        came_from = Some(b);
        match &p.terms[b] {
            Term::Goto(j) => b = *j,
            Term::Cond(cond, t, f) => b = if eval(cond, &mut st) != 0 { *t } else { *f },
            Term::Ret(e) => {
                let value = e.as_ref().map(|e| eval(e, &mut st));
                break Outcome::Returned(value);
            }
        }
    };
    Observation {
        outcome,
        mem: st.mem,
        calls: st.calls,
    }
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Option<i64>),
    Goto(String),
    OutOfFuel,
}

fn contains_label(stmts: &[PreHirStmt], label: &str) -> bool {
    stmts.iter().any(|s| stmt_contains_label(s, label))
}

fn stmt_contains_label(s: &PreHirStmt, label: &str) -> bool {
    match s {
        PreHirStmt::Label(l) => l == label,
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => contains_label(then_body, label) || contains_label(else_body, label),
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. }
        | PreHirStmt::Block(body) => contains_label(body, label),
        _ => false,
    }
}

/// Run a statement list. While `seek` holds a label, statements are skipped
/// until the one containing it, which is entered without evaluating anything
/// on the way in -- C's `goto` into a nested scope.
fn exec_list(stmts: &[PreHirStmt], st: &mut State, seek: &mut Option<String>) -> Flow {
    for s in stmts {
        if let Some(label) = seek.as_deref() {
            if !stmt_contains_label(s, label) {
                continue;
            }
        }
        match exec_stmt(s, st, seek) {
            Flow::Normal => {}
            other => return other,
        }
    }
    Flow::Normal
}

/// One pass through a loop body; `None` means keep iterating.
fn loop_body(body: &[PreHirStmt], st: &mut State, seek: &mut Option<String>) -> Option<Flow> {
    match exec_list(body, st, seek) {
        Flow::Normal | Flow::Continue => None,
        Flow::Break => Some(Flow::Normal),
        other => Some(other),
    }
}

fn exec_stmt(s: &PreHirStmt, st: &mut State, seek: &mut Option<String>) -> Flow {
    match s {
        PreHirStmt::Label(l) => {
            if seek.as_deref() == Some(l.as_str()) {
                *seek = None;
            }
            Flow::Normal
        }
        PreHirStmt::Goto(l) => Flow::Goto(l.clone()),
        PreHirStmt::Return(e) => Flow::Return(e.as_ref().map(|e| eval(e, st))),
        PreHirStmt::Break => Flow::Break,
        PreHirStmt::Continue => Flow::Continue,
        PreHirStmt::Assign { .. } | PreHirStmt::Expr(_) => {
            if !burn(st) {
                return Flow::OutOfFuel;
            }
            exec_effect(s, st);
            Flow::Normal
        }
        PreHirStmt::Block(body) => exec_list(body, st, seek),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            if let Some(label) = seek.clone() {
                return if contains_label(then_body, &label) {
                    exec_list(then_body, st, seek)
                } else {
                    exec_list(else_body, st, seek)
                };
            }
            if eval(cond, st) != 0 {
                exec_list(then_body, st, seek)
            } else {
                exec_list(else_body, st, seek)
            }
        }
        PreHirStmt::While { cond, body } => {
            if seek.is_some() {
                if let Some(flow) = loop_body(body, st, seek) {
                    return flow;
                }
            }
            loop {
                if eval(cond, st) == 0 {
                    return Flow::Normal;
                }
                if let Some(flow) = loop_body(body, st, seek) {
                    return flow;
                }
            }
        }
        PreHirStmt::DoWhile { body, cond } => loop {
            if let Some(flow) = loop_body(body, st, seek) {
                return flow;
            }
            if eval(cond, st) == 0 {
                return Flow::Normal;
            }
        },
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let entered_by_goto = seek.is_some();
            if !entered_by_goto {
                if let Some(init) = init {
                    if let Flow::OutOfFuel = exec_stmt(init, st, seek) {
                        return Flow::OutOfFuel;
                    }
                }
            }
            let mut first = true;
            loop {
                let skip_test = first && entered_by_goto;
                first = false;
                if !skip_test && cond.as_ref().is_some_and(|c| eval(c, st) == 0) {
                    return Flow::Normal;
                }
                if let Some(flow) = loop_body(body, st, seek) {
                    return flow;
                }
                if let Some(update) = update {
                    if let Flow::OutOfFuel = exec_stmt(update, st, seek) {
                        return Flow::OutOfFuel;
                    }
                }
            }
        }
        other => panic!("fixture interpreter: unsupported statement {other:?}"),
    }
}

/// Run a structured body, restarting from the top in seek mode on a `goto`.
fn run_structured(body: &[PreHirStmt], mut st: State) -> Observation {
    let mut seek: Option<String> = None;
    let outcome = loop {
        match exec_list(body, &mut st, &mut seek) {
            Flow::Goto(label) => {
                assert!(
                    contains_label(body, &label),
                    "goto {label} has no label in the structured body"
                );
                seek = Some(label);
            }
            Flow::Return(v) => break Outcome::Returned(v),
            Flow::Normal => {
                assert!(seek.is_none(), "label {seek:?} never found");
                break Outcome::Returned(None);
            }
            Flow::OutOfFuel => break Outcome::OutOfFuel,
            Flow::Break | Flow::Continue => panic!("break/continue escaped every loop"),
        }
    };
    Observation {
        outcome,
        mem: st.mem,
        calls: st.calls,
    }
}

// -------------------------------------------------------- the comparison --

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // SplitMix64: deterministic, dependency-free.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const VARS: [&str; 4] = ["a", "b", "r", "p"];

fn initial_states(seed: u64, count: usize) -> Vec<State> {
    let mut rng = Rng(seed ^ 0xA5A5_5A5A);
    (0..count)
        .map(|_| {
            let mut vars = BTreeMap::new();
            for name in VARS {
                vars.insert(name.to_string(), rng.below(8) as i64);
            }
            vars.insert("n".to_string(), 0);
            let mem = (0..MEM_CELLS).map(|i| (i, rng.below(4) as i64)).collect();
            State {
                vars,
                mem,
                calls: Vec::new(),
                fuel: FUEL,
            }
        })
        .collect()
}

/// Run SAILR on `p`; when it structures the function, every initial state
/// must be observed identically on the CFG and on the structured body.
fn check(p: &Program, seed: u64) -> Option<Vec<PreHirStmt>> {
    let mut host = p.host();
    let body = fold_sailr(&mut host).expect("mock host lowering cannot fail")?;
    for (i, st) in initial_states(seed, 24).into_iter().enumerate() {
        let want = run_cfg(p, st.clone());
        let got = run_structured(&body, st);
        assert_eq!(
            got, want,
            "seed {seed} state {i}: structured body diverges from the CFG\nprogram: {p:#?}\nbody: {body:#?}"
        );
    }
    Some(body)
}

fn count_returns(stmts: &[PreHirStmt]) -> usize {
    stmts
        .iter()
        .map(|s| match s {
            PreHirStmt::Return(_) => 1,
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => count_returns(then_body) + count_returns(else_body),
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. }
            | PreHirStmt::Block(body) => count_returns(body),
            _ => 0,
        })
        .sum()
}

fn count_gotos(stmts: &[PreHirStmt]) -> usize {
    stmts
        .iter()
        .map(|s| match s {
            PreHirStmt::Goto(_) => 1,
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => count_gotos(then_body) + count_gotos(else_body),
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. }
            | PreHirStmt::Block(body) => count_gotos(body),
            _ => 0,
        })
        .sum()
}

fn random_cond(rng: &mut Rng) -> PreHirExpr {
    use PreHirBinaryOp as B;
    match rng.below(7) {
        0 => bin(
            B::Eq,
            bin(B::And, v(VARS[rng.below(4)]), c(3)),
            c(rng.below(4) as i64),
        ),
        1 => bin(B::SLt, v("n"), c(2 + rng.below(6) as i64)),
        2 => bin(B::Ne, load(c(rng.below(8) as i64)), c(0)),
        3 => bin(B::SLt, v(VARS[rng.below(4)]), v(VARS[rng.below(4)])),
        5 => bin(B::Ne, call("probe", vec![v("a")]), c(3)),
        _ => bin(B::Eq, load(v("p")), v("a")),
    }
}

fn random_value(rng: &mut Rng) -> PreHirExpr {
    use PreHirBinaryOp as B;
    match rng.below(4) {
        0 => bin(B::Add, v(VARS[rng.below(4)]), c(rng.below(5) as i64)),
        1 => load(v(VARS[rng.below(4)])),
        2 => bin(B::Xor, v(VARS[rng.below(4)]), v("n")),
        _ => c(rng.below(9) as i64),
    }
}

/// A random CFG: mostly forward edges with some back edges (so loops,
/// nested loops and irreducible regions all occur), one to three exits that
/// many blocks branch to (merged returns), statements that write variables
/// and memory and make calls, and returns that read memory and the
/// variables each predecessor set.
fn random_program(seed: u64) -> Program {
    let mut rng = Rng(seed);
    let n = 4 + rng.below(13);
    let exits = 1 + rng.below(3.min(n - 2));
    let first_exit = n - exits;
    let mut stmts = Vec::with_capacity(n);
    let mut terms = Vec::with_capacity(n);
    for b in 0..n {
        let mut body = vec![tick()];
        if b >= first_exit {
            // A return: sometimes a plain one (copyable), sometimes with an
            // effect (must never be copied).
            match rng.below(4) {
                0 => body.push(set("r", random_value(&mut rng))),
                1 => body.push(PreHirStmt::Expr(call("effect", vec![v("r")]))),
                _ => {}
            }
            let value = match rng.below(3) {
                0 => bin(PreHirBinaryOp::Add, v("r"), load(v("p"))),
                1 => v(VARS[rng.below(4)]),
                _ => bin(PreHirBinaryOp::Add, v("n"), v("r")),
            };
            stmts.push(body);
            terms.push(Term::Ret(Some(value)));
            continue;
        }
        for _ in 0..rng.below(3) {
            body.push(match rng.below(4) {
                0 => set(VARS[rng.below(4)], random_value(&mut rng)),
                1 => store(v(VARS[rng.below(4)]), random_value(&mut rng)),
                2 => PreHirStmt::Expr(call("log", vec![v(VARS[rng.below(4)])])),
                _ => set("r", random_value(&mut rng)),
            });
        }
        let target = |rng: &mut Rng| -> usize {
            match rng.below(8) {
                // A back edge (or a self loop).
                0 => rng.below(b + 1),
                // Straight to an exit: the merged-return shape.
                1 | 2 => first_exit + rng.below(exits),
                _ => b + 1 + rng.below(n - b - 1),
            }
        };
        let t = target(&mut rng);
        let f = target(&mut rng);
        stmts.push(body);
        if rng.below(5) == 0 || t == f {
            terms.push(Term::Goto(t));
        } else {
            terms.push(Term::Cond(random_cond(&mut rng), t, f));
        }
    }
    // Some merged returns are return joins: each way in carries its own
    // value (any subset of the ways in), and the join's own statements are
    // register moves. A join holding a call is never one.
    let mut joins = Vec::new();
    for x in first_exit..n {
        let moves_only = stmts[x]
            .iter()
            .all(|s| matches!(s, PreHirStmt::Assign { lhs: PreHirLValue::Var(_), rhs } if !matches!(rhs, PreHirExpr::Call { .. })));
        let ways_in: Vec<usize> = terms
            .iter()
            .enumerate()
            .filter(|(_, term)| match term {
                Term::Goto(j) => *j == x,
                Term::Cond(_, t, f) => *t == x || *f == x,
                Term::Ret(_) => false,
            })
            .map(|(b, _)| b)
            .collect();
        // A block with one way in is not a join: its own lowering is that
        // way's value.
        if !moves_only || ways_in.len() < 2 || rng.below(3) != 0 {
            continue;
        }
        for b in ways_in {
            if rng.below(4) != 0 {
                joins.push(((b, x), random_value(&mut rng)));
            }
        }
    }
    Program {
        stmts,
        terms,
        joins,
    }
}

// ------------------------------------------------------------------ tests --

/// The comparison must be able to fail: a body that drops one
/// predecessor's assignment, or swaps a branch, is caught.
#[test]
fn the_differential_check_detects_a_wrong_body() {
    let p = phi_diamond_with_memory_return();
    let mut host = p.host();
    let body = fold_sailr(&mut host).unwrap().expect("structures");
    let states = initial_states(1, 24);
    let mut swapped = body.clone();
    let mut swapped_one = false;
    swap_first_if(&mut swapped, &mut swapped_one);
    assert!(swapped_one, "fixture has a branch to swap");
    assert!(
        states
            .iter()
            .any(|st| run_cfg(&p, st.clone()) != run_structured(&swapped, st.clone())),
        "swapping a branch must be observable"
    );
}

fn swap_first_if(stmts: &mut [PreHirStmt], done: &mut bool) {
    for s in stmts {
        if *done {
            return;
        }
        if let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = s
        {
            std::mem::swap(then_body, else_body);
            *done = true;
            return;
        }
    }
}

/// `if (a & 3 == 0) { r = a; *2 = b; if (b < 4) return r + *2; *2 = 7; }
/// else { r = b; } log(r); return r + *2;` -- the compiler's single return
/// block reads `r` (a phi of the two arms' writes) and memory one path
/// stored to. The early return leaves the `if` arm through two exits, so
/// nothing folds until the merged return is split.
///
/// 0 -> {1, 3}; 1 -> {R, 2}; 2 -> 4; 3 -> 4; 4 -> R.
fn phi_diamond_with_memory_return() -> Program {
    use PreHirBinaryOp as B;
    let r = 5;
    Program {
        stmts: vec![
            vec![tick()],
            vec![tick(), set("r", v("a")), store(c(2), v("b"))],
            vec![tick(), store(c(2), c(7))],
            vec![tick(), set("r", v("b"))],
            vec![tick(), PreHirStmt::Expr(call("log", vec![v("r")]))],
            vec![tick()],
        ],
        terms: vec![
            Term::Cond(bin(B::Eq, bin(B::And, v("a"), c(3)), c(0)), 1, 3),
            Term::Cond(bin(B::SLt, v("b"), c(4)), r, 2),
            Term::Goto(4),
            Term::Goto(4),
            Term::Goto(r),
            Term::Ret(Some(bin(B::Add, v("r"), load(c(2))))),
        ],
        joins: Vec::new(),
    }
}

#[test]
fn a_split_return_keeps_each_predecessors_values_and_memory() {
    let p = phi_diamond_with_memory_return();
    let body = check(&p, 7).expect("SAILR structures the fixture");
    assert!(
        count_returns(&body) > p.reachable_exits(),
        "the fixture must exercise the return split: {body:#?}"
    );
}

/// A return that makes a call is never copied: copying would not change any
/// one path, but it is the effect the size bound exists to keep in one place.
#[test]
fn a_return_with_a_call_is_not_copied_and_stays_equivalent() {
    let mut p = phi_diamond_with_memory_return();
    let r = p.stmts.len() - 1;
    p.stmts[r].push(PreHirStmt::Expr(call("effect", vec![v("r")])));
    if let Some(body) = check(&p, 11) {
        assert_eq!(count_returns(&body), 1, "{body:#?}");
    }
}

/// Nested loops whose inner and outer exits both reach one merged return.
///
/// 0 -> 1; 1 (outer head) -> {2, R}; 2 (inner head) -> {3, R}; 3 -> {2, 4};
/// 4 -> 1.
#[test]
fn nested_loops_exiting_to_a_shared_return_stay_equivalent() {
    use PreHirBinaryOp as B;
    let r = 5;
    let p = Program {
        stmts: vec![
            vec![tick(), set("r", c(0))],
            vec![tick(), set("b", bin(B::Add, v("b"), c(1)))],
            vec![tick(), set("r", bin(B::Add, v("r"), v("b")))],
            vec![tick(), store(v("r"), v("n"))],
            vec![tick()],
            vec![tick()],
        ],
        terms: vec![
            Term::Goto(1),
            Term::Cond(bin(B::SLt, v("b"), c(6)), 2, r),
            Term::Cond(bin(B::Ne, load(v("a")), c(3)), 3, r),
            Term::Cond(bin(B::SLt, v("r"), c(9)), 2, 4),
            Term::Goto(1),
            Term::Ret(Some(bin(B::Add, v("r"), load(c(0))))),
        ],
        joins: Vec::new(),
    };
    check(&p, 3);
}

/// An irreducible pair (two entries into one cycle) next to a merged return.
///
/// 0 -> {1, 2}; 1 -> {2, R}; 2 -> {1, R}.
#[test]
fn an_irreducible_cycle_with_a_shared_return_stays_equivalent() {
    use PreHirBinaryOp as B;
    let r = 3;
    let p = Program {
        stmts: vec![
            vec![tick()],
            vec![tick(), set("a", bin(B::Add, v("a"), c(1)))],
            vec![tick(), set("r", bin(B::Xor, v("r"), v("a")))],
            vec![tick()],
        ],
        terms: vec![
            Term::Cond(bin(B::Eq, v("b"), c(0)), 1, 2),
            Term::Cond(bin(B::SLt, v("n"), c(7)), 2, r),
            Term::Cond(bin(B::SLt, v("a"), c(5)), 1, r),
            Term::Ret(Some(v("r"))),
        ],
        joins: Vec::new(),
    };
    check(&p, 5);
}

/// A merged return whose value depends on the way in, as the builder
/// lowers it on x86-64: the shared block restores registers and moves the
/// return register, and its own `return` names the value only one
/// predecessor leaves there. `lower_return_join_expr_for_predecessor` gives
/// each predecessor's value; the existing structurer returns that value on
/// each edge instead of entering the join.
///
/// `if (!n0) return 0; do { ...; if (!--k) return acc; } while (ok);
/// u = f(); return u;` with 0 -> {1, R}; 1 -> {2, R}; 2 -> {1, 3}; 3 -> R.
fn return_join_with_values_per_predecessor() -> Program {
    use PreHirBinaryOp as B;
    let r = 4;
    Program {
        stmts: vec![
            vec![tick(), set("k", v("a"))],
            vec![tick(), set("acc", bin(B::Add, v("acc"), v("b")))],
            vec![tick(), set("k", bin(B::Sub, v("k"), c(1)))],
            vec![tick(), set("u", call("f", vec![v("acc")]))],
            // Epilogue: return-register move and callee-saved restores.
            vec![set("rax", v("u")), set("k", load(c(5))), set("acc", v("k"))],
        ],
        terms: vec![
            Term::Cond(bin(B::Ne, v("a"), c(0)), 1, r),
            Term::Cond(bin(B::Ne, v("k"), c(1)), 2, r),
            Term::Cond(bin(B::SLt, v("acc"), c(20)), 1, 3),
            Term::Goto(r),
            Term::Ret(Some(v("rax"))),
        ],
        joins: vec![((0, r), c(0)), ((1, r), v("acc")), ((3, r), v("u"))],
    }
}

/// Every edge into a return join must return the value that edge carries.
/// Before the fix SAILR emitted the join's generic body for each way in
/// (directly, as a jump target, or as a copy), returning `u` -- never
/// assigned on the first two paths -- where the code returns `0` or `acc`.
#[test]
fn each_way_into_a_return_join_returns_its_own_value() {
    let p = return_join_with_values_per_predecessor();
    check(&p, 13).expect("SAILR structures the fixture");
}

/// Thousands of random CFGs -- loops, nested loops, irreducible regions,
/// shared tails, several exits, stores, calls -- each compared on 24 initial
/// states wherever SAILR structures it. The counts at the end keep the test
/// honest: the split and the concession paths must both actually run.
#[test]
fn random_cfgs_structured_by_sailr_match_the_cfg() {
    const PROGRAMS: usize = 2000;
    let mut structured = 0usize;
    let mut with_split = 0usize;
    let mut with_goto = 0usize;
    let mut with_join = 0usize;
    let mut programs = 0usize;
    let mut seed = 0u64;
    while programs < PROGRAMS {
        seed += 1;
        let p = random_program(seed);
        if !p.all_reachable() {
            continue;
        }
        programs += 1;
        let Some(body) = check(&p, seed) else {
            continue;
        };
        structured += 1;
        with_join += usize::from(!p.joins.is_empty());
        if count_returns(&body) > p.reachable_exits() {
            with_split += 1;
        }
        if count_gotos(&body) > 0 {
            with_goto += 1;
        }
    }
    eprintln!(
        "structured {structured} / {PROGRAMS}, with split {with_split}, with goto {with_goto}, with return joins {with_join}"
    );
    assert!(structured >= 1000, "structured only {structured}");
    assert!(with_split >= 100, "split only {with_split}");
    assert!(with_goto >= 100, "goto only {with_goto}");
    assert!(with_join >= 100, "return joins only {with_join}");
}

// ------------------------------------------- edge choice: old vs new code --

/// The edge choice as first written: dominator *sets*, a reachability walk
/// per candidate edge, and an adjacency copy per H2 score. Kept as the
/// reference the faster implementation must agree with.
mod reference {
    use super::super::*;
    use crate::cfg_analysis::DomTree;
    use crate::reaching_driver::survives_without;

    pub(super) fn pick_edge(
        graph: &CollapseGraph,
        failed: Option<&Shape>,
        mut is_plain_return: impl FnMut(NodeId) -> bool,
    ) -> Option<(NodeId, NodeId)> {
        let head = live_node_owning_entry(graph)?;
        let live: Vec<NodeId> = graph.live_nodes().collect();
        let dense = Dense::new(graph, head, &live);
        let dom = DomTree::analyze(&dense.succ, &dense.pred);
        let mut unrelated = Vec::new();
        let mut to_dominator = Vec::new();
        let mut from_dominator = Vec::new();
        for &u in &live {
            if graph.successors(u).len() > 2 {
                continue;
            }
            for &v in graph.successors(u) {
                if u == v || (v != head && graph.predecessors(v).len() < 2) {
                    continue;
                }
                let (du, dv) = (dense.index[&u], dense.index[&v]);
                let u_dom_v = dom.dominates(du, dv);
                let v_dom_u = dom.dominates(dv, du);
                let class = if !u_dom_v && !v_dom_u {
                    &mut unrelated
                } else if !u_dom_v {
                    &mut to_dominator
                } else {
                    &mut from_dominator
                };
                class.push((u, v));
            }
        }
        let near_failure = |e: &(NodeId, NodeId)| {
            failed.is_some_and(|s| s.members.contains(&e.0) || s.members.contains(&e.1))
        };
        for class in [unrelated, to_dominator, from_dominator] {
            if class.is_empty() {
                continue;
            }
            let focused: Vec<_> = class.iter().copied().filter(near_failure).collect();
            let edges = if focused.is_empty() { class } else { focused };
            for edge in order_edges(graph, &dense, edges, &mut is_plain_return) {
                if survives_without(graph, head, &live, edge.0, edge.1) {
                    return Some(edge);
                }
            }
        }
        None
    }

    fn order_edges(
        graph: &CollapseGraph,
        dense: &Dense,
        mut edges: Vec<(NodeId, NodeId)>,
        is_plain_return: &mut impl FnMut(NodeId) -> bool,
    ) -> Vec<(NodeId, NodeId)> {
        let block = |n: NodeId| graph.node(n).map_or(usize::MAX, |x| x.entry_block);
        let mut cand_in: HashMap<NodeId, usize> = HashMap::default();
        for &(_, v) in &edges {
            *cand_in.entry(v).or_default() += 1;
        }
        edges.sort_by_key(|&(u, v)| (dense.rpo[&v], cand_in[&v], block(u), block(v)));
        let siblings = |v: NodeId| graph.predecessors(v).len().saturating_sub(1);
        let h1 = edges
            .iter()
            .map(|&(_, v)| siblings(v))
            .filter(|&c| c > 0)
            .min();
        let mut best: Vec<(NodeId, NodeId)> = match h1 {
            Some(min) => edges
                .iter()
                .copied()
                .filter(|&(_, v)| siblings(v) == min)
                .collect(),
            None => edges.clone(),
        };
        if best.len() > 1 && edges.len() <= H2_MAX_EDGES && dense.succ.len() <= H2_MAX_NODES {
            let scored: Vec<((NodeId, NodeId), usize)> = best
                .iter()
                .map(|&(u, v)| {
                    let (du, dv) = (dense.index[&u], dense.index[&v]);
                    let mut succ = dense.succ.clone();
                    let mut pred = dense.pred.clone();
                    succ[du].retain(|&x| x != dv);
                    pred[dv].retain(|&x| x != du);
                    let ipdom = ImmPostDomTree::compute(&succ, &pred);
                    let count = (0..succ.len())
                        .filter(|&n| ipdom.immediate_postdominator(n).is_some())
                        .count();
                    ((u, v), count)
                })
                .collect();
            let max = scored.iter().map(|(_, c)| *c).max().unwrap_or(0);
            best = scored
                .into_iter()
                .filter(|(_, c)| *c == max)
                .map(|(e, _)| e)
                .collect();
        }
        if best.len() > 1 {
            let to_return: Vec<_> = best
                .iter()
                .copied()
                .filter(|&(_, v)| is_plain_return(v))
                .collect();
            if !to_return.is_empty() {
                best = to_return;
            }
        }
        let mut out = best.clone();
        out.extend(edges.into_iter().filter(|e| !best.contains(e)));
        out
    }
}

/// A random graph shaped like the ones the driver sees: a fall-through
/// spine, a second edge on most nodes (forward, back, or to one of one to
/// three exits), and -- when allowed -- edges that leave nodes unreachable.
fn random_graph(rng: &mut Rng, n: usize, allow_unreachable: bool) -> Vec<Vec<usize>> {
    loop {
        let exits = 1 + rng.below(3.min(n - 1));
        let first_exit = n - exits;
        let mut succ = vec![Vec::new(); n];
        for (b, out) in succ.iter_mut().enumerate().take(first_exit) {
            let spine = if b + 1 < first_exit {
                b + 1
            } else {
                first_exit + rng.below(exits)
            };
            if !(allow_unreachable && rng.below(8) == 0) {
                out.push(spine);
            }
            if rng.below(4) != 0 {
                let t = match rng.below(6) {
                    0 => rng.below(b + 1),
                    1 | 2 => first_exit + rng.below(exits),
                    _ => b + 1 + rng.below(n - b - 1),
                };
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        if allow_unreachable || reaches_everything(&succ) {
            return succ;
        }
    }
}

fn reaches_everything(succ: &[Vec<usize>]) -> bool {
    let mut seen = vec![false; succ.len()];
    let mut stack = vec![0];
    seen[0] = true;
    while let Some(n) = stack.pop() {
        for &s in &succ[n] {
            if !seen[s] {
                seen[s] = true;
                stack.push(s);
            }
        }
    }
    seen.iter().all(|&x| x)
}

/// The faster edge choice picks exactly the edge the reference picks, on
/// graphs as they stand mid-structuring: fresh, after folds and splits, with
/// a failed shape focusing the choice, with unreachable nodes.
#[test]
fn pick_edge_matches_the_reference_implementation() {
    let mut rng = Rng(0x5A11);
    let mut compared = 0usize;
    let mut picked = 0usize;
    for case in 0..4000 {
        let n = 3 + rng.below(if case % 10 == 0 { 120 } else { 30 });
        let succ = random_graph(&mut rng, n, case % 7 == 0);
        let mut graph = CollapseGraph::from_cfg(&succ);
        // Fold a few shapes first, as the driver would have.
        for _ in 0..rng.below(4) {
            let Some(shape) = find_shape(&graph) else {
                break;
            };
            if !fold_accounts_for_every_internal_edge(&graph, &shape) {
                break;
            }
            if graph
                .collapse(&shape.members, shape.entry, Vec::new())
                .is_err()
            {
                break;
            }
        }
        let failed = find_shape(&graph).filter(|_| rng.below(2) == 0);
        let plain = rng.next();
        let is_plain = |v: NodeId| (plain >> (v % 64)) & 1 == 1;
        let fast = pick_edge(
            &graph,
            failed.as_ref(),
            &mut WorkMeter::new(u64::MAX),
            is_plain,
        );
        let slow = reference::pick_edge(&graph, failed.as_ref(), is_plain);
        assert_eq!(fast, slow, "case {case}: {succ:?}");
        compared += 1;
        picked += usize::from(fast.is_some());
    }
    assert!(picked > compared / 2, "picked {picked} of {compared}");
}

/// A function that would keep the driver busy is declined once the
/// aggregate budget is spent, the same way every time.
#[test]
fn a_spent_work_budget_declines_deterministically() {
    let p = phi_diamond_with_memory_return();
    let mut host = p.host();
    assert!(
        fold_sailr_inner(&mut host, &mut WorkMeter::new(u64::MAX))
            .unwrap()
            .is_ok()
    );
    for _ in 0..2 {
        let mut host = p.host();
        let out = fold_sailr_inner(&mut host, &mut WorkMeter::new(3)).unwrap();
        assert!(
            matches!(&out, Err(reason) if reason.starts_with("work budget")),
            "{out:?}"
        );
    }
}

/// Timing of the edge choice, old against new, on large graphs: the best of
/// five runs over the same 20 graphs per size. Run with `--ignored
/// --nocapture` in an optimized profile.
#[test]
#[ignore]
fn pick_edge_timing() {
    fn best_of<T>(mut run: impl FnMut() -> T) -> (T, f64) {
        let mut best = f64::INFINITY;
        let mut out = None;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            out = Some(run());
            best = best.min(t.elapsed().as_secs_f64());
        }
        (out.expect("ran"), best)
    }
    for &n in &[50usize, 200, 800, 2000, 5000] {
        let mut rng = Rng(n as u64);
        let graphs: Vec<CollapseGraph> = (0..20)
            .map(|_| CollapseGraph::from_cfg(&random_graph(&mut rng, n, false)))
            .collect();
        let (fast, fast_time) = best_of(|| {
            graphs
                .iter()
                .map(|g| pick_edge(g, None, &mut WorkMeter::new(u64::MAX), |_| false))
                .collect::<Vec<_>>()
        });
        let (slow, slow_time) = best_of(|| {
            graphs
                .iter()
                .map(|g| reference::pick_edge(g, None, |_| false))
                .collect::<Vec<_>>()
        });
        assert_eq!(fast, slow);
        eprintln!(
            "n={n:5}: reference {:>10.3} ms/graph, new {:>8.3} ms/graph",
            slow_time * 1e3 / 20.0,
            fast_time * 1e3 / 20.0
        );
    }
}
