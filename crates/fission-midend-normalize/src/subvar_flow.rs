use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Details of a wide variable being replaced with a narrower one.
#[derive(Debug, Clone)]
struct ReplaceVar {
    original_name: String,
    mask: u64,
    bitsize: u32,
    new_name: String,
    new_type: NirType,
}

#[derive(Debug, Clone)]
struct AssignInfo {
    lhs: String,
    rhs: PreHirExpr,
}

/// Helper to identify if a bitmask represents a standard narrow subvariable size.
fn is_valid_subvar_mask(mask: u64) -> bool {
    mask == 0xff || mask == 0xffff || mask == 0xffffffff
}

/// Computes bitsize and flowsize (byte size) for a standard bitmask.
fn compute_sizes(mask: u64) -> Option<(u32, u32)> {
    if mask == 0 {
        return None;
    }
    let low = mask.trailing_zeros();
    let high = 64 - mask.leading_zeros();
    let bitsize = high - low;
    let flowsize = if bitsize <= 8 {
        1
    } else if bitsize <= 16 {
        2
    } else if bitsize <= 32 {
        4
    } else if bitsize <= 64 {
        8
    } else {
        return None;
    };
    Some((bitsize, flowsize))
}

/// Instrumentation counters for subvariable-flow index construction and rewriting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SubvarFlowStats {
    pub index_builds: usize,
    pub indexed_stmt_visits: usize,
    pub indexed_use_queries: usize,
    pub rewrite_passes: usize,
    pub candidates_evaluated: usize,
    pub candidates_applied: usize,
}

/// Deterministic helper collecting distinct variable names in `expr` in a single subtree pass.
fn collect_distinct_vars_in_expr<'a>(
    expr: &'a PreHirExpr,
    out: &mut Vec<&'a str>,
    seen: &mut HashSet<&'a str>,
) {
    match expr {
        PreHirExpr::Var(name) => {
            if seen.insert(name.as_str()) {
                out.push(name.as_str());
            }
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            collect_distinct_vars_in_expr(expr, out, seen);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_distinct_vars_in_expr(lhs, out, seen);
            collect_distinct_vars_in_expr(rhs, out, seen);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_distinct_vars_in_expr(arg, out, seen);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_distinct_vars_in_expr(base, out, seen);
            collect_distinct_vars_in_expr(index, out, seen);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_distinct_vars_in_expr(cond, out, seen);
            collect_distinct_vars_in_expr(then_expr, out, seen);
            collect_distinct_vars_in_expr(else_expr, out, seen);
        }
        _ => {}
    }
}

fn expr_var_set(expr: &PreHirExpr) -> Vec<String> {
    let mut vars = Vec::new();
    let mut seen = HashSet::default();
    collect_distinct_vars_in_expr(expr, &mut vars, &mut seen);
    vars.into_iter().map(str::to_string).collect()
}

/// Revision-local definition and contextual use-site index built in a single O(N + U) HIR pass.
struct SubvarHirIndex {
    assigns: Vec<AssignInfo>,
    def_counts: HashMap<String, usize>,
    def_map: HashMap<String, PreHirExpr>,
    multi_defined: HashSet<String>,
    uses_by_var: HashMap<String, Vec<UseInfo>>,
    co_occurring_vars: HashMap<String, HashSet<String>>,
    stmt_visits: usize,
}

impl SubvarHirIndex {
    fn build(stmts: &[PreHirStmt]) -> Self {
        let mut idx = Self {
            assigns: Vec::new(),
            def_counts: HashMap::default(),
            def_map: HashMap::default(),
            multi_defined: HashSet::default(),
            uses_by_var: HashMap::default(),
            co_occurring_vars: HashMap::default(),
            stmt_visits: 0,
        };
        idx.visit_stmts(stmts, true);
        for assign in &idx.assigns {
            if !idx.multi_defined.contains(&assign.lhs) {
                idx.def_map.insert(assign.lhs.clone(), assign.rhs.clone());
            }
        }
        idx
    }

    fn rebuild_uses(&mut self, stmts: &[PreHirStmt]) {
        self.uses_by_var.clear();
        self.co_occurring_vars.clear();
        self.stmt_visits = 0;
        self.visit_stmts(stmts, false);
    }

    fn record_co_occurring(&mut self, vars: &[String]) {
        if vars.len() <= 1 {
            return;
        }
        for v in vars {
            let entry = self.co_occurring_vars.entry(v.clone()).or_default();
            for other in vars {
                if other != v {
                    entry.insert(other.clone());
                }
            }
        }
    }

    fn push_use(&mut self, var_name: &str, context: UseContext) {
        self.uses_by_var
            .entry(var_name.to_string())
            .or_default()
            .push(UseInfo { context });
    }

    fn push_binary_operand_use(
        &mut self,
        var_name: &str,
        op: PreHirBinaryOp,
        left_var: bool,
        right_var: bool,
        other: &PreHirExpr,
        dest: Option<&str>,
    ) {
        let _ = left_var;
        match op {
            PreHirBinaryOp::Eq
            | PreHirBinaryOp::Ne
            | PreHirBinaryOp::Lt
            | PreHirBinaryOp::Le
            | PreHirBinaryOp::Gt
            | PreHirBinaryOp::Ge
            | PreHirBinaryOp::SLt
            | PreHirBinaryOp::SLe
            | PreHirBinaryOp::SGt
            | PreHirBinaryOp::SGe => {
                self.push_use(
                    var_name,
                    UseContext::Compare {
                        op,
                        other: other.clone(),
                    },
                );
            }
            PreHirBinaryOp::And => {
                if let (Some(d), PreHirExpr::Const(mask, _)) = (dest, other) {
                    self.push_use(
                        var_name,
                        UseContext::AndMask {
                            mask: *mask as u64,
                            dest: d.to_string(),
                        },
                    );
                } else if let Some(d) = dest {
                    self.push_use(
                        var_name,
                        UseContext::Binary {
                            op,
                            other: other.clone(),
                            dest: d.to_string(),
                        },
                    );
                } else {
                    self.push_use(var_name, UseContext::Incompatible);
                }
            }
            PreHirBinaryOp::Add
            | PreHirBinaryOp::Sub
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
            | PreHirBinaryOp::Shl
            | PreHirBinaryOp::Shr
            | PreHirBinaryOp::Sar => {
                if let Some(d) = dest {
                    if matches!(
                        op,
                        PreHirBinaryOp::Shl | PreHirBinaryOp::Shr | PreHirBinaryOp::Sar
                    ) && right_var
                    {
                        self.push_use(
                            var_name,
                            UseContext::ShiftAmount {
                                op,
                                dest: d.to_string(),
                            },
                        );
                    } else {
                        self.push_use(
                            var_name,
                            UseContext::Binary {
                                op,
                                other: other.clone(),
                                dest: d.to_string(),
                            },
                        );
                    }
                } else {
                    self.push_use(var_name, UseContext::Incompatible);
                }
            }
            _ => {
                self.push_use(var_name, UseContext::Incompatible);
            }
        }
    }

    fn index_expr_use(
        &mut self,
        expr: &PreHirExpr,
        dest: Option<&str>,
        out_vars: &mut Vec<String>,
    ) {
        match expr {
            PreHirExpr::Var(name) => {
                out_vars.push(name.clone());
                if let Some(d) = dest {
                    self.push_use(
                        name,
                        UseContext::Binary {
                            op: PreHirBinaryOp::And,
                            other: PreHirExpr::Const(-1, NirType::Unknown),
                            dest: d.to_string(),
                        },
                    );
                } else {
                    self.push_use(name, UseContext::Incompatible);
                }
            }
            PreHirExpr::Binary { op, lhs, rhs, .. } => {
                let left_name = match &**lhs {
                    PreHirExpr::Var(name) => Some(name.as_str()),
                    _ => None,
                };
                let right_name = match &**rhs {
                    PreHirExpr::Var(name) => Some(name.as_str()),
                    _ => None,
                };

                if let Some(lname) = left_name {
                    let both_same = right_name == Some(lname);
                    self.push_binary_operand_use(lname, *op, true, both_same, rhs, dest);
                }
                if let Some(rname) = right_name {
                    if left_name != Some(rname) {
                        self.push_binary_operand_use(rname, *op, false, true, lhs, dest);
                    }
                }

                let vars = expr_var_set(expr);
                for v in &vars {
                    if left_name != Some(v.as_str()) && right_name != Some(v.as_str()) {
                        self.push_use(v, UseContext::Incompatible);
                    }
                }
                out_vars.extend(vars);
            }
            PreHirExpr::Cast { ty, expr: inner } => {
                if let PreHirExpr::Var(name) = &**inner {
                    out_vars.push(name.clone());
                    if let Some(d) = dest {
                        self.push_use(
                            name,
                            UseContext::Cast {
                                target_ty: ty.clone(),
                                dest: d.to_string(),
                            },
                        );
                    } else {
                        self.push_use(name, UseContext::Incompatible);
                    }
                    return;
                }
                let vars = expr_var_set(inner);
                for v in &vars {
                    self.push_use(v, UseContext::Incompatible);
                }
                out_vars.extend(vars);
            }
            PreHirExpr::Call { target, args, .. } => {
                for (arg_idx, arg) in args.iter().enumerate() {
                    if let PreHirExpr::Var(name) = arg {
                        out_vars.push(name.clone());
                        self.push_use(
                            name,
                            UseContext::Call {
                                target: target.clone(),
                                arg_idx,
                            },
                        );
                        continue;
                    }
                    let vars = expr_var_set(arg);
                    for v in &vars {
                        self.push_use(v, UseContext::Incompatible);
                    }
                    out_vars.extend(vars);
                }
            }
            _ => {
                let vars = expr_var_set(expr);
                for v in &vars {
                    self.push_use(v, UseContext::Incompatible);
                }
                out_vars.extend(vars);
            }
        }
    }

    fn index_lvalue_use(&mut self, lhs: &PreHirLValue, out_vars: &mut Vec<String>) {
        let vars = match lhs {
            PreHirLValue::Var(_) => Vec::new(),
            PreHirLValue::Deref { ptr, .. } => expr_var_set(ptr),
            PreHirLValue::Index { base, index, .. } => {
                let mut seen = HashSet::default();
                let mut collected = Vec::new();
                for v in expr_var_set(base).into_iter().chain(expr_var_set(index)) {
                    if seen.insert(v.clone()) {
                        collected.push(v);
                    }
                }
                collected
            }
            PreHirLValue::FieldAccess { base, .. } => expr_var_set(base),
        };
        for v in &vars {
            self.push_use(v, UseContext::Incompatible);
        }
        out_vars.extend(vars);
    }

    fn visit_stmts(&mut self, stmts: &[PreHirStmt], collect_defs: bool) {
        for stmt in stmts {
            self.stmt_visits += 1;
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs,
                } => {
                    if collect_defs {
                        let count = self.def_counts.entry(name.clone()).or_insert(0);
                        *count += 1;
                        if *count > 1 {
                            self.multi_defined.insert(name.clone());
                        }
                        self.assigns.push(AssignInfo {
                            lhs: name.clone(),
                            rhs: rhs.clone(),
                        });
                    }
                    let mut vars = vec![name.clone()];
                    self.index_expr_use(rhs, Some(name), &mut vars);
                    self.record_co_occurring(&vars);
                }
                PreHirStmt::Assign { lhs, rhs } => {
                    let mut vars = Vec::new();
                    self.index_lvalue_use(lhs, &mut vars);
                    self.index_expr_use(rhs, None, &mut vars);
                    self.record_co_occurring(&vars);
                }
                PreHirStmt::Expr(expr) => {
                    let mut vars = Vec::new();
                    self.index_expr_use(expr, None, &mut vars);
                    self.record_co_occurring(&vars);
                }
                PreHirStmt::Return(Some(expr)) => {
                    if let PreHirExpr::Var(name) = expr {
                        self.push_use(name, UseContext::Return);
                    } else {
                        let vars = expr_var_set(expr);
                        for v in &vars {
                            self.push_use(v, UseContext::Incompatible);
                        }
                        self.record_co_occurring(&vars);
                    }
                }
                PreHirStmt::Return(None) => {}
                PreHirStmt::Block(body) => {
                    self.visit_stmts(body, collect_defs);
                }
                PreHirStmt::While { cond, body } => {
                    let mut vars = Vec::new();
                    self.index_expr_use(cond, None, &mut vars);
                    self.record_co_occurring(&vars);
                    self.visit_stmts(body, collect_defs);
                }
                PreHirStmt::DoWhile { body, cond } => {
                    self.visit_stmts(body, collect_defs);
                    let mut vars = Vec::new();
                    self.index_expr_use(cond, None, &mut vars);
                    self.record_co_occurring(&vars);
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    if let Some(i) = init {
                        self.visit_stmts(std::slice::from_ref(i.as_ref()), collect_defs);
                    }
                    if let Some(c) = cond {
                        let mut vars = Vec::new();
                        self.index_expr_use(c, None, &mut vars);
                        self.record_co_occurring(&vars);
                    }
                    if let Some(u) = update {
                        self.visit_stmts(std::slice::from_ref(u.as_ref()), collect_defs);
                    }
                    self.visit_stmts(body, collect_defs);
                }
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    let mut vars = Vec::new();
                    self.index_expr_use(cond, None, &mut vars);
                    self.record_co_occurring(&vars);
                    self.visit_stmts(then_body, collect_defs);
                    self.visit_stmts(else_body, collect_defs);
                }
                PreHirStmt::Switch {
                    expr,
                    cases,
                    default,
                } => {
                    let mut vars = Vec::new();
                    self.index_expr_use(expr, None, &mut vars);
                    self.record_co_occurring(&vars);
                    for case in cases {
                        self.visit_stmts(&case.body, collect_defs);
                    }
                    self.visit_stmts(default, collect_defs);
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone)]
struct UseInfo {
    context: UseContext,
}

#[derive(Debug, Clone)]
enum UseContext {
    AndMask {
        mask: u64,
        dest: String,
    },
    Cast {
        target_ty: NirType,
        dest: String,
    },
    Binary {
        op: PreHirBinaryOp,
        other: PreHirExpr,
        dest: String,
    },
    Compare {
        op: PreHirBinaryOp,
        other: PreHirExpr,
    },
    ShiftAmount {
        op: PreHirBinaryOp,
        dest: String,
    },
    Call {
        target: String,
        arg_idx: usize,
    },
    Return,
    Incompatible,
}

/// RuleSubvarShift: trace SUBPIECE/CONCAT-style `Or` reassembly back to the source varnode.
fn trace_or_subvar_piece(lhs: &PreHirExpr, rhs: &PreHirExpr, mask: u64) -> Option<(String, u64)> {
    let (low_part, high_part) = match (lhs, rhs) {
        (
            low,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                ..
            },
        ) => (low, rhs),
        (
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                ..
            },
            low,
        ) => (low, lhs),
        _ => return None,
    };
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs: shifted,
        rhs: shift_amt,
        ..
    } = high_part
    else {
        return None;
    };
    let PreHirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 || *n >= 64 {
        return None;
    }
    let expected_low_mask = (1u64 << *n).saturating_sub(1);
    if mask != expected_low_mask {
        return None;
    }
    let (src, and_mask) = match low_part {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => match (and_lhs.as_ref(), and_rhs.as_ref()) {
            (PreHirExpr::Var(name), PreHirExpr::Const(m, _)) => (name.clone(), *m as u64),
            (PreHirExpr::Const(m, _), PreHirExpr::Var(name)) => (name.clone(), *m as u64),
            _ => return None,
        },
        _ => return None,
    };
    if and_mask != expected_low_mask {
        return None;
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
        lhs: shr_lhs,
        rhs: shr_amt,
        ..
    } = shifted.as_ref()
    else {
        return None;
    };
    let PreHirExpr::Const(shr_n, _) = shr_amt.as_ref() else {
        return None;
    };
    if shr_n != n {
        return None;
    }
    match shr_lhs.as_ref() {
        PreHirExpr::Var(name) if name == &src => Some((src, mask)),
        _ => None,
    }
}

/// Global Subvariable Flow solver executing worklist bit constraint propagation backward and forward.
struct SubvarFlowSolver<'a> {
    def_map: &'a HashMap<String, PreHirExpr>,
    type_map: &'a HashMap<String, NirType>,
    multi_defined: &'a HashSet<String>,
    /// Bits each variable may hold nonzero (`compute_nz_masks`).
    nz_masks: &'a HashMap<String, u64>,
    varmap: HashMap<String, ReplaceVar>,
    pull_points: HashSet<String>,
    worklist: Vec<(String, u64)>,
    pull_count: usize,
}

impl<'a> SubvarFlowSolver<'a> {
    fn solve(
        &mut self,
        body: &mut Vec<PreHirStmt>,
        index: &mut SubvarHirIndex,
        pending_varmap: &mut HashMap<String, ReplaceVar>,
        stale_vars: &mut HashSet<String>,
        stats: &mut SubvarFlowStats,
    ) -> bool {
        while let Some((var_name, mask)) = self.worklist.pop() {
            if let Some(existing) = self.varmap.get(&var_name) {
                if existing.mask != mask {
                    return false;
                }
                continue;
            }

            let (bitsize, _) = match compute_sizes(mask) {
                Some(sz) => sz,
                None => return false,
            };

            let is_signed = if let Some(ty) = self.type_map.get(&var_name) {
                match ty {
                    NirType::Int { signed, .. } => *signed,
                    _ => false,
                }
            } else {
                false
            };

            let new_name = format!("{}_sub{}", var_name, bitsize);
            let new_type = NirType::Int {
                bits: bitsize,
                signed: is_signed,
            };
            self.varmap.insert(
                var_name.clone(),
                ReplaceVar {
                    original_name: var_name.clone(),
                    mask,
                    bitsize,
                    new_name,
                    new_type,
                },
            );

            if !self.trace_backward(&var_name, mask) {
                return false;
            }

            if !self.trace_forward(
                body,
                index,
                pending_varmap,
                stale_vars,
                stats,
                &var_name,
                mask,
            ) {
                return false;
            }
        }
        self.pull_count > 0
    }

    fn trace_backward(&mut self, var_name: &str, mask: u64) -> bool {
        if self.multi_defined.contains(var_name) {
            return false;
        }
        let def_expr = match self.def_map.get(var_name) {
            Some(expr) => expr,
            // A name with no assignment anywhere in the function is only a
            // safe leaf to narrow-and-rename if it's a genuinely declared
            // local/param (`type_map` is seeded from exactly those, see
            // `apply_subvar_flow_pass`) -- renaming it fabricates a new
            // `func.locals` entry with no initializer (see the push loop in
            // `apply_subvar_flow_pass`), which is fine for a real parameter
            // but produces a bogus, uninitialized-looking declaration for a
            // synthetic named value that isn't backed by any real storage
            // (e.g. a fixed-address field read materialized as a bare,
            // deliberately-unregistered `PreHirExpr::Var`, as in the Windows
            // TEB/PEB field recognition in `fission-pcode`).
            None => return self.type_map.contains_key(var_name),
        };

        match def_expr {
            PreHirExpr::Binary { op, lhs, rhs, .. } => match op {
                PreHirBinaryOp::Or => {
                    if let Some((src, piece_mask)) = trace_or_subvar_piece(lhs, rhs, mask) {
                        self.worklist.push((src, piece_mask));
                        return true;
                    }
                    if let PreHirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let PreHirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                PreHirBinaryOp::And | PreHirBinaryOp::Xor => {
                    if let PreHirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let PreHirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                PreHirBinaryOp::Add | PreHirBinaryOp::Sub => {
                    if (mask & 1) == 0 {
                        return false;
                    }
                    if let PreHirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let PreHirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                PreHirBinaryOp::Shl => {
                    if let PreHirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask >> sa;
                            if new_mask == 0 {
                                true
                            } else if (new_mask << sa) == mask {
                                if let PreHirExpr::Var(l) = &**lhs {
                                    self.worklist.push((l.clone(), new_mask));
                                }
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                PreHirBinaryOp::Shr | PreHirBinaryOp::Sar => {
                    if let PreHirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask << sa;
                            if (new_mask >> sa) == mask {
                                if let PreHirExpr::Var(l) = &**lhs {
                                    self.worklist.push((l.clone(), new_mask));
                                }
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            },
            PreHirExpr::Cast { expr, .. } => {
                if let PreHirExpr::Var(inner) = &**expr {
                    self.worklist.push((inner.clone(), mask));
                    true
                } else {
                    false
                }
            }
            PreHirExpr::Unary { op, expr, .. } => {
                if *op == PreHirUnaryOp::BitNot || (*op == PreHirUnaryOp::Neg && (mask & 1) != 0) {
                    if let PreHirExpr::Var(inner) = &**expr {
                        self.worklist.push((inner.clone(), mask));
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            PreHirExpr::Var(src) => {
                self.worklist.push((src.clone(), mask));
                true
            }
            PreHirExpr::Const(_, _) => true,
            PreHirExpr::Load { .. } | PreHirExpr::Call { .. } => true,
            _ => false,
        }
    }

    fn trace_forward(
        &mut self,
        body: &mut [PreHirStmt],
        index: &mut SubvarHirIndex,
        pending_varmap: &mut HashMap<String, ReplaceVar>,
        stale_vars: &mut HashSet<String>,
        stats: &mut SubvarFlowStats,
        var_name: &str,
        mask: u64,
    ) -> bool {
        // A variable already mapped in `pending_varmap` has every occurrence in
        // `func.body` renamed away by `rewrite_stmts`, so it has zero remaining
        // uses in the rewritten HIR without needing an index rebuild.
        let empty = Vec::new();
        let uses = if pending_varmap.contains_key(var_name) {
            stats.indexed_use_queries += 1;
            empty
        } else {
            if stale_vars.contains(var_name) {
                if !pending_varmap.is_empty() {
                    rewrite_stmts(body, pending_varmap);
                    stats.rewrite_passes += 1;
                    pending_varmap.clear();
                }
                index.rebuild_uses(body);
                stats.index_builds += 1;
                stats.indexed_stmt_visits += index.stmt_visits;
                stale_vars.clear();
            }
            stats.indexed_use_queries += 1;
            index.uses_by_var.get(var_name).cloned().unwrap_or_default()
        };

        for use_info in uses {
            match use_info.context {
                UseContext::AndMask { mask: m, dest } => {
                    if m == mask {
                        self.pull_points.insert(dest.clone());
                        self.pull_count += 1;
                        if let Some(rep) = self.varmap.get(var_name).cloned() {
                            self.varmap.insert(dest, rep);
                        }
                    } else {
                        return false;
                    }
                }
                UseContext::Cast { target_ty, dest } => {
                    if (mask & 1) == 0 {
                        return false;
                    }
                    let bits = match target_ty {
                        NirType::Int { bits, .. } => bits,
                        NirType::Bool => 1,
                        _ => return false,
                    };
                    let (bitsize, _) = match compute_sizes(mask) {
                        Some(sz) => sz,
                        None => return false,
                    };
                    if bits == bitsize {
                        self.pull_points.insert(dest.clone());
                        self.pull_count += 1;
                        if let Some(rep) = self.varmap.get(var_name).cloned() {
                            self.varmap.insert(dest, rep);
                        }
                    } else {
                        return false;
                    }
                }
                UseContext::Binary { op, other, dest } => match op {
                    PreHirBinaryOp::Add | PreHirBinaryOp::Sub => {
                        if (mask & 1) == 0 {
                            return false;
                        }
                        self.worklist.push((dest.clone(), mask));
                        if let PreHirExpr::Var(o) = other {
                            self.worklist.push((o.clone(), mask));
                        }
                    }
                    PreHirBinaryOp::And | PreHirBinaryOp::Or | PreHirBinaryOp::Xor => {
                        self.worklist.push((dest.clone(), mask));
                        if let PreHirExpr::Var(o) = other {
                            self.worklist.push((o.clone(), mask));
                        }
                    }
                    _ => return false,
                },
                UseContext::Compare { op, other } => {
                    if !matches!(op, PreHirBinaryOp::Eq | PreHirBinaryOp::Ne) {
                        return false;
                    }
                    // The comparison reads the whole variable, so the narrow
                    // copy compares the same only when every bit outside the
                    // mask is known zero, the mask starts at bit 0 (the
                    // constant is not shifted), and the copy is unsigned (a
                    // signed `i8` holding 200 compares as -56).
                    let upper_bits_zero = self
                        .nz_masks
                        .get(var_name)
                        .is_some_and(|nz| nz & !mask == 0);
                    let signed = matches!(
                        self.type_map.get(var_name),
                        Some(NirType::Int { signed: true, .. })
                    );
                    if !upper_bits_zero || signed || (mask & 1) == 0 {
                        return false;
                    }
                    match other {
                        PreHirExpr::Const(val, _) => {
                            if (val as u64 & !mask) != 0 {
                                return false;
                            }
                        }
                        PreHirExpr::Var(o) => {
                            self.worklist.push((o.clone(), mask));
                        }
                        _ => return false,
                    }
                    self.pull_count += 1;
                }
                _ => return false,
            }
        }
        true
    }
}

/// Recursively applies subvariable replacements to a given expression.
fn rewrite_expr(expr: &mut PreHirExpr, varmap: &HashMap<String, ReplaceVar>) {
    match expr {
        PreHirExpr::Cast { ty, expr: inner } => {
            if let PreHirExpr::Var(name) = &**inner {
                if let Some(rep) = varmap.get(name) {
                    if let NirType::Int { bits, .. } = ty {
                        if *bits == rep.bitsize {
                            *expr = PreHirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }
            rewrite_expr(inner, varmap);
        }
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            if *op == PreHirBinaryOp::And {
                if let (PreHirExpr::Var(name), PreHirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = PreHirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
                if let (PreHirExpr::Const(mask, _), PreHirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = PreHirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }

            let l_narrow_ty = if let PreHirExpr::Var(n) = &**lhs {
                varmap.get(n).map(|r| r.new_type.clone())
            } else {
                None
            };
            let r_narrow_ty = if let PreHirExpr::Var(n) = &**rhs {
                varmap.get(n).map(|r| r.new_type.clone())
            } else {
                None
            };

            rewrite_expr(lhs, varmap);
            rewrite_expr(rhs, varmap);

            if let Some(nty) = l_narrow_ty.or(r_narrow_ty) {
                *ty = nty;
            }
        }
        PreHirExpr::Unary {
            expr: inner, ty, ..
        } => {
            let narrow_ty = if let PreHirExpr::Var(name) = &**inner {
                varmap.get(name).map(|rep| rep.new_type.clone())
            } else {
                None
            };
            rewrite_expr(inner, varmap);
            if let Some(nty) = narrow_ty {
                *ty = nty;
            }
        }
        PreHirExpr::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        _ => {
            for_each_child_expr(expr, |child| rewrite_expr(child, varmap));
        }
    }
}

fn for_each_child_expr<F>(expr: &mut PreHirExpr, mut f: F)
where
    F: FnMut(&mut PreHirExpr),
{
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            f(inner);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                f(arg);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            f(cond);
            f(then_expr);
            f(else_expr);
        }
        _ => {}
    }
}

fn rewrite_lvalue(lhs: &mut PreHirLValue, varmap: &HashMap<String, ReplaceVar>) {
    match lhs {
        PreHirLValue::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        PreHirLValue::Deref { ptr, .. } => {
            rewrite_expr(ptr, varmap);
        }
        PreHirLValue::Index { base, index, .. } => {
            rewrite_expr(base, varmap);
            rewrite_expr(index, varmap);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            rewrite_expr(base, varmap);
        }
    }
}

fn rewrite_stmt(stmt: &mut PreHirStmt, varmap: &HashMap<String, ReplaceVar>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            rewrite_lvalue(lhs, varmap);
            rewrite_expr(rhs, varmap);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            rewrite_expr(expr, varmap);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            rewrite_expr(va_list, varmap);
        }
        PreHirStmt::Block(body) => {
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), varmap);
        }
        PreHirStmt::While { cond, body } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), varmap);
        }
        PreHirStmt::DoWhile { body, cond } => {
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), varmap);
            rewrite_expr(cond, varmap);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                rewrite_stmt(i.as_mut(), varmap);
            }
            if let Some(c) = cond {
                rewrite_expr(c, varmap);
            }
            if let Some(u) = update {
                rewrite_stmt(u.as_mut(), varmap);
            }
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), varmap);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), varmap);
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), varmap);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_expr(expr, varmap);
            for case in cases {
                rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    varmap,
                );
            }
            rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), varmap);
        }
        _ => {}
    }
}

fn rewrite_stmts(stmts: &mut [PreHirStmt], varmap: &HashMap<String, ReplaceVar>) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(stmt, varmap);
    }
}

/// Pipeline entry point returning both change status and indexing/rewrite metrics.
pub(crate) fn apply_subvar_flow_pass_with_stats(
    func: &mut PreHirFunction,
) -> (bool, SubvarFlowStats) {
    let mut stats = SubvarFlowStats::default();
    let mut index = SubvarHirIndex::build(&func.body);
    stats.index_builds = 1;
    stats.indexed_stmt_visits = index.stmt_visits;

    let mut type_map = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let nz_masks = crate::global_opt::compute_nz_masks(func);
    let mut candidate_set = HashSet::default();

    for assign in &index.assigns {
        if index.multi_defined.contains(&assign.lhs) {
            continue;
        }
        match &assign.rhs {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                if let (PreHirExpr::Var(name), PreHirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
                if let (PreHirExpr::Const(mask, _), PreHirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
            }
            PreHirExpr::Cast { ty, expr } => {
                if let PreHirExpr::Var(name) = &**expr {
                    if let NirType::Int { bits, .. } = ty {
                        if *bits == 8 || *bits == 16 || *bits == 32 {
                            let mask = (1u64 << bits) - 1;
                            candidate_set.insert((name.clone(), mask));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    for (name, mask) in &nz_masks {
        if is_valid_subvar_mask(*mask) {
            if let Some(ty) = type_map.get(name) {
                if let NirType::Int { bits: w, .. } = ty {
                    if let Some((bitsize, _)) = compute_sizes(*mask) {
                        if *w > bitsize {
                            candidate_set.insert((name.clone(), *mask));
                        }
                    }
                }
            }
        }
    }

    let mut candidates: Vec<(String, u64)> = candidate_set.into_iter().collect();
    candidates.sort();

    if candidates.is_empty() {
        return (false, stats);
    }

    let def_map = std::mem::take(&mut index.def_map);
    let multi_defined = std::mem::take(&mut index.multi_defined);
    let mut local_names: HashSet<String> = func.locals.iter().map(|l| l.name.clone()).collect();
    let mut pending_varmap: HashMap<String, ReplaceVar> = HashMap::default();
    let mut stale_vars: HashSet<String> = HashSet::default();
    let mut changed = false;

    for (var_name, mask) in candidates {
        stats.candidates_evaluated += 1;
        let mut solver = SubvarFlowSolver {
            def_map: &def_map,
            type_map: &type_map,
            multi_defined: &multi_defined,
            nz_masks: &nz_masks,
            varmap: HashMap::default(),
            pull_points: HashSet::default(),
            worklist: vec![(var_name, mask)],
            pull_count: 0,
        };

        if solver.solve(
            &mut func.body,
            &mut index,
            &mut pending_varmap,
            &mut stale_vars,
            &mut stats,
        ) {
            stats.candidates_applied += 1;
            let mut sorted_replacements: Vec<(&String, &ReplaceVar)> =
                solver.varmap.iter().collect();
            sorted_replacements.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (_, replace_var) in sorted_replacements {
                if local_names.insert(replace_var.new_name.clone()) {
                    // A leaf nothing in the body assigns (a formal, typically) is
                    // read through the new name, so the new name must be given its
                    // value: the low bits of the original. Without this the
                    // replacement is a declaration nobody initializes.
                    let leaf = !def_map.contains_key(&replace_var.original_name);
                    let binding = PreHirBinding {
                        name: replace_var.new_name.clone(),
                        ty: replace_var.new_type.clone(),
                        surface_type_name: None,
                        origin: Some(NirBindingOrigin::Temp),
                        initializer: leaf.then(|| PreHirExpr::Cast {
                            ty: replace_var.new_type.clone(),
                            expr: Box::new(PreHirExpr::Var(replace_var.original_name.clone())),
                        }),
                    };
                    func.locals.push(binding);
                }
            }

            // If this solution collides with buffered rewrites in `pending_varmap`, flush
            // buffered rewrites first before merging, then mark only affected co-occurring
            // variables stale so independent components require zero index rebuilds.
            let conflicts_with_pending =
                solver.varmap.keys().any(|k| pending_varmap.contains_key(k));
            if conflicts_with_pending && !pending_varmap.is_empty() {
                rewrite_stmts(&mut func.body, &pending_varmap);
                stats.rewrite_passes += 1;
                pending_varmap.clear();
                index.rebuild_uses(&func.body);
                stats.index_builds += 1;
                stats.indexed_stmt_visits += index.stmt_visits;
                stale_vars.clear();
            }

            for (orig, rep) in solver.varmap {
                stale_vars.insert(orig.clone());
                stale_vars.insert(rep.new_name.clone());
                if let Some(neighbors) = index.co_occurring_vars.get(&orig) {
                    stale_vars.extend(neighbors.iter().cloned());
                }
                pending_varmap.insert(orig, rep);
            }
            changed = true;
        }
    }

    if !pending_varmap.is_empty() {
        rewrite_stmts(&mut func.body, &pending_varmap);
        stats.rewrite_passes += 1;
    }

    (changed, stats)
}

/// Pipeline entry point for the Global Subvariable Flow Analyzer normalization pass.
pub fn apply_subvar_flow_pass(func: &mut PreHirFunction) -> bool {
    apply_subvar_flow_pass_with_stats(func).0
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn u8_ty() -> NirType {
        NirType::Int {
            bits: 8,
            signed: false,
        }
    }

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    /// A formal narrowed through its uses is read under the new name; that name must
    /// be given the formal's low bits, or the function reads a variable nothing sets.
    #[test]
    fn a_narrowed_formal_leaf_is_initialized_from_the_formal() {
        let mut func = PreHirFunction::default();
        func.name = "leaf_formal".to_string();
        func.params.push(PreHirBinding {
            name: "a".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        });
        for name in ["y", "z"] {
            func.locals.push(PreHirBinding {
                name: name.to_string(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            });
        }
        // y = a & 0xff;  z = y == 12;
        func.body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("y".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("a".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                    ty: u64_ty(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("z".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(PreHirExpr::Var("y".to_string())),
                    rhs: Box::new(PreHirExpr::Const(12, u64_ty())),
                    ty: NirType::Bool,
                },
            },
        ];

        assert!(apply_subvar_flow_pass(&mut func));

        let narrowed = func
            .locals
            .iter()
            .find(|binding| binding.name == "a_sub8")
            .expect("the formal's low byte gets a name");
        assert_eq!(
            narrowed.initializer,
            Some(PreHirExpr::Cast {
                ty: u8_ty(),
                expr: Box::new(PreHirExpr::Var("a".to_string())),
            }),
            "{narrowed:?}"
        );
        // A name the body assigns keeps no initializer: its value comes from the body.
        let assigned = func.locals.iter().find(|binding| binding.name == "y_sub8");
        assert!(
            assigned.is_none_or(|binding| binding.initializer.is_none()),
            "{assigned:?}"
        );
    }

    #[test]
    fn test_subvar_flow_rewrite() {
        let mut func = PreHirFunction::default();
        func.name = "test_subflow".to_string();

        // `a`/`b` are function parameters (real, declared storage feeding
        // `x = a + b`) -- must be registered like any genuine binding, or
        // the def-less leaf case in `trace_backward` now conservatively
        // refuses to narrow them (an unregistered name reaching that path
        // is treated as a synthetic value with no real storage to declare,
        // not a parameter -- see the Windows TEB/PEB field regression this
        // guarded against).
        func.params.push(PreHirBinding {
            name: "a".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        });
        func.params.push(PreHirBinding {
            name: "b".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(1)),
            initializer: None,
        });
        func.locals.push(PreHirBinding {
            name: "x".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(PreHirBinding {
            name: "y".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(PreHirBinding {
            name: "z".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });

        // Construct mock body:
        // x = a + b;
        // y = x & 0xff;
        // z = y == 12;
        let stmt1 = PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("a".to_string())),
                rhs: Box::new(PreHirExpr::Var("b".to_string())),
                ty: u64_ty(),
            },
        };
        let stmt2 = PreHirStmt::Assign {
            lhs: PreHirLValue::Var("y".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                ty: u64_ty(),
            },
        };
        let stmt3 = PreHirStmt::Assign {
            lhs: PreHirLValue::Var("z".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs: Box::new(PreHirExpr::Var("y".to_string())),
                rhs: Box::new(PreHirExpr::Const(12, u64_ty())),
                ty: NirType::Bool,
            },
        };

        func.body = vec![stmt1, stmt2, stmt3];

        let changed = apply_subvar_flow_pass(&mut func);
        assert!(changed);

        // Verify z comparison is bridged and y's masking AND is completely eliminated
        if let PreHirStmt::Assign { rhs, .. } = &func.body[2] {
            if let PreHirExpr::Binary { lhs, rhs, .. } = rhs {
                if let PreHirExpr::Var(name) = &**lhs {
                    assert_eq!(name, "x_sub8");
                } else {
                    panic!("LHS should be narrow variable x_sub8");
                }
                if let PreHirExpr::Const(val, _) = &**rhs {
                    assert_eq!(*val, 12);
                } else {
                    panic!("RHS should be constant 12");
                }
            } else {
                panic!("Statement 3 should be a binary comparison");
            }
        } else {
            panic!("Statement 3 should be an assignment");
        }
    }

    /// `x = a + b; y = x & 0xff; z = x == 12;` -- the comparison reads all
    /// of `x`, whose upper bits are not known zero: `(u8)x == 12` is a
    /// different test. Equality is no safer than ordering here.
    #[test]
    fn subvar_flow_keeps_an_equality_compare_on_a_variable_with_live_upper_bits() {
        let binding = |name: &str, origin: NirBindingOrigin| PreHirBinding {
            name: name.to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };
        let var = |n: &str| Box::new(PreHirExpr::Var(n.to_string()));
        let assign = |lhs: &str, op, l: Box<PreHirExpr>, r: Box<PreHirExpr>| PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs.to_string()),
            rhs: PreHirExpr::Binary {
                op,
                lhs: l,
                rhs: r,
                ty: u64_ty(),
            },
        };
        let body = |compared: &str| {
            vec![
                assign("x", PreHirBinaryOp::Add, var("a"), var("b")),
                assign(
                    "y",
                    PreHirBinaryOp::And,
                    var("x"),
                    Box::new(PreHirExpr::Const(0xff, u64_ty())),
                ),
                assign(
                    "z",
                    PreHirBinaryOp::Eq,
                    var(compared),
                    Box::new(PreHirExpr::Const(12, u64_ty())),
                ),
            ]
        };
        let func = |compared: &str| PreHirFunction {
            params: vec![
                binding("a", NirBindingOrigin::ParamIndex(0)),
                binding("b", NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![
                binding("x", NirBindingOrigin::Temp),
                binding("y", NirBindingOrigin::Temp),
                binding("z", NirBindingOrigin::Temp),
            ],
            body: body(compared),
            ..Default::default()
        };

        let mut whole = func("x");
        apply_subvar_flow_pass(&mut whole);
        let PreHirStmt::Assign { rhs, .. } = &whole.body[2] else {
            panic!("{:#?}", whole.body);
        };
        assert!(
            matches!(rhs, PreHirExpr::Binary { lhs, .. } if matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == "x")),
            "{rhs:#?}"
        );

        // Control: comparing the masked `y` (upper bits zero) still narrows.
        let mut masked = func("y");
        assert!(apply_subvar_flow_pass(&mut masked));
        let PreHirStmt::Assign { rhs, .. } = &masked.body[2] else {
            panic!("{:#?}", masked.body);
        };
        assert!(
            matches!(rhs, PreHirExpr::Binary { lhs, .. } if matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == "x_sub8")),
            "{rhs:#?}"
        );
    }

    #[test]
    fn subvar_flow_rejects_ordering_compare_and_shifted_add() {
        let binding = |name: &str, ty: NirType, origin: NirBindingOrigin| PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };

        // Ordering compare (<) must reject subvar narrowing when used on the masked value.
        let mut cmp_func = PreHirFunction {
            params: vec![
                binding("a", u64_ty(), NirBindingOrigin::ParamIndex(0)),
                binding("b", u64_ty(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![
                binding("x", u64_ty(), NirBindingOrigin::Temp),
                binding("y", u64_ty(), NirBindingOrigin::Temp),
                binding("z", u64_ty(), NirBindingOrigin::Temp),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".to_string())),
                        rhs: Box::new(PreHirExpr::Var("b".to_string())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("z".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Lt,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(12, u64_ty())),
                        ty: NirType::Bool,
                    },
                },
            ],
            ..Default::default()
        };
        assert!(!apply_subvar_flow_pass(&mut cmp_func));

        // Shifted high-byte mask (0xff00) must not propagate across Add (low-bit carry-in).
        let mut add_func = PreHirFunction {
            params: vec![
                binding("a", u64_ty(), NirBindingOrigin::ParamIndex(0)),
                binding("b", u64_ty(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![
                binding("x", u64_ty(), NirBindingOrigin::Temp),
                binding("y", u64_ty(), NirBindingOrigin::Temp),
                binding("z", u64_ty(), NirBindingOrigin::Temp),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".to_string())),
                        rhs: Box::new(PreHirExpr::Var("b".to_string())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(0xff00, u64_ty())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("z".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("y".to_string())),
                        rhs: Box::new(PreHirExpr::Const(0x1200, u64_ty())),
                        ty: NirType::Bool,
                    },
                },
            ],
            ..Default::default()
        };
        assert!(!apply_subvar_flow_pass(&mut add_func));
    }

    /// The whole-pass test above cannot reach a shifted mask: candidates are
    /// 0xff/0xffff/0xffffffff, and the only way to shift one is a `>>` def,
    /// whose operand then fails its own forward trace. Drive the solver
    /// directly so the carry guard itself is pinned: bits 8..15 of `a + b`
    /// depend on the carry out of bits 0..7, which a 0xff00 slice drops.
    #[test]
    fn subvar_flow_carry_ops_reject_a_mask_that_skips_bit_zero() {
        let var = |n: &str| Box::new(PreHirExpr::Var(n.to_string()));
        let type_map = HashMap::from_iter([
            ("a".to_string(), u64_ty()),
            ("b".to_string(), u64_ty()),
            ("x".to_string(), u64_ty()),
        ]);
        let multi_defined = HashSet::default();
        let nz_masks = HashMap::default();
        let check_back = |def_map: &HashMap<String, PreHirExpr>, mask: u64| {
            SubvarFlowSolver {
                def_map,
                type_map: &type_map,
                multi_defined: &multi_defined,
                nz_masks: &nz_masks,
                varmap: HashMap::default(),
                pull_points: HashSet::default(),
                worklist: Vec::new(),
                pull_count: 0,
            }
            .trace_backward("x", mask)
        };
        let binary = |op| PreHirExpr::Binary {
            op,
            lhs: var("a"),
            rhs: var("b"),
            ty: u64_ty(),
        };
        for op in [PreHirBinaryOp::Add, PreHirBinaryOp::Sub] {
            let def_map = HashMap::from_iter([("x".to_string(), binary(op))]);
            assert!(!check_back(&def_map, 0xff00), "{op:?}");
            assert!(check_back(&def_map, 0xff), "{op:?}");
        }
        let neg = PreHirExpr::Unary {
            op: PreHirUnaryOp::Neg,
            expr: var("a"),
            ty: u64_ty(),
        };
        let neg_map = HashMap::from_iter([("x".to_string(), neg)]);
        assert!(!check_back(&neg_map, 0xff00));
        assert!(check_back(&neg_map, 0xff));
        // Bitwise ops have no carry: any slice is fine.
        let xor_map = HashMap::from_iter([("x".to_string(), binary(PreHirBinaryOp::Xor))]);
        assert!(check_back(&xor_map, 0xff00));
    }

    // --- Legacy O(N^2) / whole-tree reference implementation for differential validation ---

    fn legacy_collect_assignments(
        stmts: &[PreHirStmt],
        assigns: &mut Vec<AssignInfo>,
        multi_def: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs,
                } => {
                    if assigns.iter().any(|a| &a.lhs == name) {
                        multi_def.insert(name.clone());
                    }
                    assigns.push(AssignInfo {
                        lhs: name.clone(),
                        rhs: rhs.clone(),
                    });
                }
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => {
                    legacy_collect_assignments(body, assigns, multi_def);
                }
                PreHirStmt::For {
                    init, update, body, ..
                } => {
                    if let Some(i) = init {
                        legacy_collect_assignments(
                            std::slice::from_ref(i.as_ref()),
                            assigns,
                            multi_def,
                        );
                    }
                    if let Some(u) = update {
                        legacy_collect_assignments(
                            std::slice::from_ref(u.as_ref()),
                            assigns,
                            multi_def,
                        );
                    }
                    legacy_collect_assignments(body, assigns, multi_def);
                }
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    legacy_collect_assignments(then_body, assigns, multi_def);
                    legacy_collect_assignments(else_body, assigns, multi_def);
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        legacy_collect_assignments(&case.body, assigns, multi_def);
                    }
                    legacy_collect_assignments(default, assigns, multi_def);
                }
                _ => {}
            }
        }
    }

    fn legacy_expr_contains_var(expr: &PreHirExpr, var_name: &str, visits: &mut usize) -> bool {
        *visits += 1;
        match expr {
            PreHirExpr::Var(name) => name == var_name,
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => {
                legacy_expr_contains_var(expr, var_name, visits)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                legacy_expr_contains_var(lhs, var_name, visits)
                    || legacy_expr_contains_var(rhs, var_name, visits)
            }
            PreHirExpr::Call { args, .. } => args
                .iter()
                .any(|arg| legacy_expr_contains_var(arg, var_name, visits)),
            PreHirExpr::Index { base, index, .. } => {
                legacy_expr_contains_var(base, var_name, visits)
                    || legacy_expr_contains_var(index, var_name, visits)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                legacy_expr_contains_var(cond, var_name, visits)
                    || legacy_expr_contains_var(then_expr, var_name, visits)
                    || legacy_expr_contains_var(else_expr, var_name, visits)
            }
            _ => false,
        }
    }

    fn legacy_find_uses(
        stmts: &[PreHirStmt],
        var_name: &str,
        uses: &mut Vec<UseInfo>,
        stmt_scans: &mut usize,
        expr_visits: &mut usize,
    ) {
        for stmt in stmts {
            *stmt_scans += 1;
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(dest),
                    rhs,
                } => legacy_analyze_expr_use(rhs, var_name, Some(dest), uses, expr_visits),
                PreHirStmt::Assign { lhs, rhs } => {
                    match lhs {
                        PreHirLValue::Var(_) => {}
                        PreHirLValue::Deref { ptr, .. } => {
                            if legacy_expr_contains_var(ptr, var_name, expr_visits) {
                                uses.push(UseInfo {
                                    context: UseContext::Incompatible,
                                });
                            }
                        }
                        PreHirLValue::Index { base, index, .. } => {
                            if legacy_expr_contains_var(base, var_name, expr_visits)
                                || legacy_expr_contains_var(index, var_name, expr_visits)
                            {
                                uses.push(UseInfo {
                                    context: UseContext::Incompatible,
                                });
                            }
                        }
                        PreHirLValue::FieldAccess { base, .. } => {
                            if legacy_expr_contains_var(base, var_name, expr_visits) {
                                uses.push(UseInfo {
                                    context: UseContext::Incompatible,
                                });
                            }
                        }
                    }
                    legacy_analyze_expr_use(rhs, var_name, None, uses, expr_visits);
                }
                PreHirStmt::Expr(expr) => {
                    legacy_analyze_expr_use(expr, var_name, None, uses, expr_visits)
                }
                PreHirStmt::Return(Some(expr)) => {
                    if legacy_expr_contains_var(expr, var_name, expr_visits) {
                        if let PreHirExpr::Var(name) = expr {
                            if name == var_name {
                                uses.push(UseInfo {
                                    context: UseContext::Return,
                                });
                                continue;
                            }
                        }
                        uses.push(UseInfo {
                            context: UseContext::Incompatible,
                        });
                    }
                }
                PreHirStmt::Block(body) => {
                    legacy_find_uses(body, var_name, uses, stmt_scans, expr_visits)
                }
                PreHirStmt::While { cond, body } => {
                    legacy_analyze_expr_use(cond, var_name, None, uses, expr_visits);
                    legacy_find_uses(body, var_name, uses, stmt_scans, expr_visits);
                }
                PreHirStmt::DoWhile { body, cond } => {
                    legacy_find_uses(body, var_name, uses, stmt_scans, expr_visits);
                    legacy_analyze_expr_use(cond, var_name, None, uses, expr_visits);
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    if let Some(i) = init {
                        legacy_find_uses(
                            std::slice::from_ref(i.as_ref()),
                            var_name,
                            uses,
                            stmt_scans,
                            expr_visits,
                        );
                    }
                    if let Some(c) = cond {
                        legacy_analyze_expr_use(c, var_name, None, uses, expr_visits);
                    }
                    if let Some(u) = update {
                        legacy_find_uses(
                            std::slice::from_ref(u.as_ref()),
                            var_name,
                            uses,
                            stmt_scans,
                            expr_visits,
                        );
                    }
                    legacy_find_uses(body, var_name, uses, stmt_scans, expr_visits);
                }
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    legacy_analyze_expr_use(cond, var_name, None, uses, expr_visits);
                    legacy_find_uses(then_body, var_name, uses, stmt_scans, expr_visits);
                    legacy_find_uses(else_body, var_name, uses, stmt_scans, expr_visits);
                }
                PreHirStmt::Switch {
                    expr,
                    cases,
                    default,
                } => {
                    legacy_analyze_expr_use(expr, var_name, None, uses, expr_visits);
                    for case in cases {
                        legacy_find_uses(&case.body, var_name, uses, stmt_scans, expr_visits);
                    }
                    legacy_find_uses(default, var_name, uses, stmt_scans, expr_visits);
                }
                _ => {}
            }
        }
    }

    fn legacy_analyze_expr_use(
        expr: &PreHirExpr,
        var_name: &str,
        dest: Option<&str>,
        uses: &mut Vec<UseInfo>,
        expr_visits: &mut usize,
    ) {
        if !legacy_expr_contains_var(expr, var_name, expr_visits) {
            return;
        }
        match expr {
            PreHirExpr::Var(name) if name == var_name => {
                if let Some(d) = dest {
                    uses.push(UseInfo {
                        context: UseContext::Binary {
                            op: PreHirBinaryOp::And,
                            other: PreHirExpr::Const(-1, NirType::Unknown),
                            dest: d.to_string(),
                        },
                    });
                } else {
                    uses.push(UseInfo {
                        context: UseContext::Incompatible,
                    });
                }
            }
            PreHirExpr::Binary { op, lhs, rhs, .. } => {
                let left_var = matches!(&**lhs, PreHirExpr::Var(name) if name == var_name);
                let right_var = matches!(&**rhs, PreHirExpr::Var(name) if name == var_name);
                if left_var || right_var {
                    let other = if left_var { &**rhs } else { &**lhs };
                    let mut dummy = SubvarHirIndex {
                        assigns: Vec::new(),
                        def_counts: HashMap::default(),
                        def_map: HashMap::default(),
                        multi_defined: HashSet::default(),
                        uses_by_var: HashMap::default(),
                        co_occurring_vars: HashMap::default(),
                        stmt_visits: 0,
                    };
                    dummy.push_binary_operand_use(var_name, *op, left_var, right_var, other, dest);
                    if let Some(u) = dummy.uses_by_var.remove(var_name) {
                        uses.extend(u);
                    }
                } else {
                    uses.push(UseInfo {
                        context: UseContext::Incompatible,
                    });
                }
            }
            PreHirExpr::Cast { ty, expr } => {
                if let PreHirExpr::Var(name) = &**expr {
                    if name == var_name {
                        if let Some(d) = dest {
                            uses.push(UseInfo {
                                context: UseContext::Cast {
                                    target_ty: ty.clone(),
                                    dest: d.to_string(),
                                },
                            });
                        } else {
                            uses.push(UseInfo {
                                context: UseContext::Incompatible,
                            });
                        }
                        return;
                    }
                }
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
            PreHirExpr::Call { target, args, .. } => {
                for (idx, arg) in args.iter().enumerate() {
                    if let PreHirExpr::Var(name) = arg {
                        if name == var_name {
                            uses.push(UseInfo {
                                context: UseContext::Call {
                                    target: target.clone(),
                                    arg_idx: idx,
                                },
                            });
                            continue;
                        }
                    }
                    if legacy_expr_contains_var(arg, var_name, expr_visits) {
                        uses.push(UseInfo {
                            context: UseContext::Incompatible,
                        });
                    }
                }
            }
            _ => uses.push(UseInfo {
                context: UseContext::Incompatible,
            }),
        }
    }

    fn legacy_apply_subvar_flow_pass(func: &mut PreHirFunction) -> (bool, usize, usize) {
        let mut assigns = Vec::new();
        let mut multi_defined = HashSet::default();
        legacy_collect_assignments(&func.body, &mut assigns, &mut multi_defined);

        let mut type_map = HashMap::default();
        for binding in func.params.iter().chain(func.locals.iter()) {
            type_map.insert(binding.name.clone(), binding.ty.clone());
        }
        let nz_masks = crate::global_opt::compute_nz_masks(func);
        let mut candidate_set = HashSet::default();
        for assign in &assigns {
            if multi_defined.contains(&assign.lhs) {
                continue;
            }
            match &assign.rhs {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs,
                    rhs,
                    ..
                } => {
                    if let (PreHirExpr::Var(name), PreHirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                        if is_valid_subvar_mask(*mask as u64) {
                            candidate_set.insert((name.clone(), *mask as u64));
                        }
                    }
                    if let (PreHirExpr::Const(mask, _), PreHirExpr::Var(name)) = (&**lhs, &**rhs) {
                        if is_valid_subvar_mask(*mask as u64) {
                            candidate_set.insert((name.clone(), *mask as u64));
                        }
                    }
                }
                PreHirExpr::Cast { ty, expr } => {
                    if let PreHirExpr::Var(name) = &**expr {
                        if let NirType::Int { bits, .. } = ty {
                            if *bits == 8 || *bits == 16 || *bits == 32 {
                                candidate_set.insert((name.clone(), (1u64 << bits) - 1));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for (name, mask) in &nz_masks {
            if is_valid_subvar_mask(*mask) {
                if let Some(NirType::Int { bits: w, .. }) = type_map.get(name) {
                    if let Some((bitsize, _)) = compute_sizes(*mask) {
                        if *w > bitsize {
                            candidate_set.insert((name.clone(), *mask));
                        }
                    }
                }
            }
        }
        let mut candidates: Vec<(String, u64)> = candidate_set.into_iter().collect();
        candidates.sort();
        if candidates.is_empty() {
            return (false, 0, 0);
        }
        let mut def_map = HashMap::default();
        for assign in assigns {
            if !multi_defined.contains(&assign.lhs) {
                def_map.insert(assign.lhs, assign.rhs);
            }
        }
        let mut changed = false;
        let mut total_stmt_scans = 0usize;
        let mut total_expr_visits = 0usize;
        for (var_name, mask) in candidates {
            let mut solver = SubvarFlowSolver {
                def_map: &def_map,
                type_map: &type_map,
                multi_defined: &multi_defined,
                nz_masks: &nz_masks,
                varmap: HashMap::default(),
                pull_points: HashSet::default(),
                worklist: vec![(var_name, mask)],
                pull_count: 0,
            };
            let ok = {
                let mut ok = true;
                while let Some((vn, m)) = solver.worklist.pop() {
                    if let Some(ex) = solver.varmap.get(&vn) {
                        if ex.mask != m {
                            ok = false;
                            break;
                        }
                        continue;
                    }
                    let Some((bitsize, _)) = compute_sizes(m) else {
                        ok = false;
                        break;
                    };
                    let is_signed = matches!(
                        solver.type_map.get(&vn),
                        Some(NirType::Int { signed: true, .. })
                    );
                    let new_name = format!("{vn}_sub{bitsize}");
                    solver.varmap.insert(
                        vn.clone(),
                        ReplaceVar {
                            original_name: vn.clone(),
                            mask: m,
                            bitsize,
                            new_name,
                            new_type: NirType::Int {
                                bits: bitsize,
                                signed: is_signed,
                            },
                        },
                    );
                    if !solver.trace_backward(&vn, m) {
                        ok = false;
                        break;
                    }
                    let mut uses = Vec::new();
                    legacy_find_uses(
                        &func.body,
                        &vn,
                        &mut uses,
                        &mut total_stmt_scans,
                        &mut total_expr_visits,
                    );
                    for u in uses {
                        match u.context {
                            UseContext::AndMask { mask: mm, dest } => {
                                if mm == m {
                                    solver.pull_points.insert(dest.clone());
                                    solver.pull_count += 1;
                                    if let Some(rep) = solver.varmap.get(&vn).cloned() {
                                        solver.varmap.insert(dest, rep);
                                    }
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            UseContext::Cast { target_ty, dest } => {
                                if (m & 1) == 0 {
                                    ok = false;
                                    break;
                                }
                                let bits = match target_ty {
                                    NirType::Int { bits, .. } => bits,
                                    NirType::Bool => 1,
                                    _ => {
                                        ok = false;
                                        break;
                                    }
                                };
                                if compute_sizes(m).is_some_and(|(bs, _)| bs == bits) {
                                    solver.pull_points.insert(dest.clone());
                                    solver.pull_count += 1;
                                    if let Some(rep) = solver.varmap.get(&vn).cloned() {
                                        solver.varmap.insert(dest, rep);
                                    }
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            UseContext::Binary { op, other, dest } => match op {
                                PreHirBinaryOp::Add | PreHirBinaryOp::Sub => {
                                    if (m & 1) == 0 {
                                        ok = false;
                                        break;
                                    }
                                    solver.worklist.push((dest.clone(), m));
                                    if let PreHirExpr::Var(o) = other {
                                        solver.worklist.push((o, m));
                                    }
                                }
                                PreHirBinaryOp::And | PreHirBinaryOp::Or | PreHirBinaryOp::Xor => {
                                    solver.worklist.push((dest.clone(), m));
                                    if let PreHirExpr::Var(o) = other {
                                        solver.worklist.push((o, m));
                                    }
                                }
                                _ => {
                                    ok = false;
                                    break;
                                }
                            },
                            UseContext::Compare { op, other } => {
                                let upper_bits_zero =
                                    solver.nz_masks.get(&vn).is_some_and(|nz| nz & !m == 0);
                                let signed = matches!(
                                    solver.type_map.get(&vn),
                                    Some(NirType::Int { signed: true, .. })
                                );
                                if !matches!(op, PreHirBinaryOp::Eq | PreHirBinaryOp::Ne)
                                    || !upper_bits_zero
                                    || signed
                                    || (m & 1) == 0
                                {
                                    ok = false;
                                    break;
                                }
                                match other {
                                    PreHirExpr::Const(val, _) => {
                                        if (val as u64 & !m) != 0 {
                                            ok = false;
                                            break;
                                        }
                                    }
                                    PreHirExpr::Var(o) => solver.worklist.push((o, m)),
                                    _ => {
                                        ok = false;
                                        break;
                                    }
                                }
                                solver.pull_count += 1;
                            }
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        break;
                    }
                }
                ok && solver.pull_count > 0
            };
            if ok {
                let mut sorted_replacements: Vec<(&String, &ReplaceVar)> =
                    solver.varmap.iter().collect();
                sorted_replacements.sort_by(|(a, _), (b, _)| a.cmp(b));
                for (_, rep) in sorted_replacements {
                    if !func.locals.iter().any(|l| l.name == rep.new_name) {
                        let leaf = !def_map.contains_key(&rep.original_name);
                        func.locals.push(PreHirBinding {
                            name: rep.new_name.clone(),
                            ty: rep.new_type.clone(),
                            surface_type_name: None,
                            origin: Some(NirBindingOrigin::Temp),
                            initializer: leaf.then(|| PreHirExpr::Cast {
                                ty: rep.new_type.clone(),
                                expr: Box::new(PreHirExpr::Var(rep.original_name.clone())),
                            }),
                        });
                    }
                }
                rewrite_stmts(&mut func.body, &solver.varmap);
                changed = true;
            }
        }
        (changed, total_stmt_scans, total_expr_visits)
    }

    #[test]
    fn indexed_pass_matches_legacy_reference_on_multidef_nested_and_edge_cases() {
        let b = |name: &str, origin| PreHirBinding {
            name: name.to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };
        let mut func = PreHirFunction {
            name: "mixed_edges".to_string(),
            params: vec![
                b("p0", NirBindingOrigin::ParamIndex(0)),
                b("p1", NirBindingOrigin::ParamIndex(1)),
                b("q0", NirBindingOrigin::ParamIndex(2)),
                b("q1", NirBindingOrigin::ParamIndex(3)),
            ],
            locals: vec![
                b("m", NirBindingOrigin::Temp),
                b("a", NirBindingOrigin::Temp),
                b("y", NirBindingOrigin::Temp),
                b("z", NirBindingOrigin::Temp),
                b("c1", NirBindingOrigin::Temp),
                b("c2", NirBindingOrigin::Temp),
                b("r", NirBindingOrigin::Temp),
            ],
            body: vec![
                // Multi-def variable `m` must be rejected cleanly
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("m".into()),
                    rhs: PreHirExpr::Var("p0".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("m".into()),
                    rhs: PreHirExpr::Var("p1".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(PreHirExpr::Var("m".into())),
                        rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                        ty: u64_ty(),
                    },
                },
                // Valid narrowing chain (`a -> c1 -> c2`) alongside an `If` branch containing
                // nested binary/call/return expressions over unrelated local `r`.
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("a".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Xor,
                        lhs: Box::new(PreHirExpr::Var("q0".into())),
                        rhs: Box::new(PreHirExpr::Var("q1".into())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("c1".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                        ty: u64_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("c2".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("c1".into())),
                        rhs: Box::new(PreHirExpr::Const(5, u64_ty())),
                        ty: NirType::Bool,
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Gt,
                        lhs: Box::new(PreHirExpr::Var("r".into())),
                        rhs: Box::new(PreHirExpr::Const(0, u64_ty())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                        target: "sink".into(),
                        args: vec![PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(PreHirExpr::Var("r".into())),
                            rhs: Box::new(PreHirExpr::Const(1, u64_ty())),
                            ty: u64_ty(),
                        }],
                        ty: NirType::Unknown,
                        origin: None,
                    })]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("r".into()))),
            ],
            ..Default::default()
        };

        let mut legacy_func = func.clone();
        let (legacy_changed, _, _) = legacy_apply_subvar_flow_pass(&mut legacy_func);
        let (indexed_changed, stats) = apply_subvar_flow_pass_with_stats(&mut func);

        assert_eq!(indexed_changed, legacy_changed);
        assert!(indexed_changed);
        assert_eq!(
            format!("{:?}", func.locals),
            format!("{:?}", legacy_func.locals)
        );
        assert_eq!(
            format!("{:?}", func.body),
            format!("{:?}", legacy_func.body)
        );
        assert!(stats.indexed_use_queries > 0);
    }

    #[test]
    fn scaling_doubling_independent_assignments_and_copy_chains() {
        let b = |name: &str, origin| PreHirBinding {
            name: name.to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };

        // 1. Doubling-size independent assignments: (y_i = x_i & 0xff; z_i = y_i == 1)
        for n in [64usize, 128, 256, 512, 1024] {
            let mut params = Vec::with_capacity(n);
            let mut locals = Vec::with_capacity(2 * n);
            let mut body = Vec::with_capacity(2 * n);
            for i in 0..n {
                params.push(b(&format!("x_{i}"), NirBindingOrigin::ParamIndex(i)));
                locals.push(b(&format!("y_{i}"), NirBindingOrigin::Temp));
                locals.push(b(&format!("z_{i}"), NirBindingOrigin::Temp));
                body.push(PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(format!("y_{i}")),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(PreHirExpr::Var(format!("x_{i}"))),
                        rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                        ty: u64_ty(),
                    },
                });
                body.push(PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(format!("z_{i}")),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var(format!("y_{i}"))),
                        rhs: Box::new(PreHirExpr::Const(1, u64_ty())),
                        ty: NirType::Bool,
                    },
                });
            }
            let base = PreHirFunction {
                name: format!("indep_{n}"),
                params,
                locals,
                body,
                ..Default::default()
            };

            let mut indexed_func = base.clone();
            let t0 = std::time::Instant::now();
            let (changed, stats) = apply_subvar_flow_pass_with_stats(&mut indexed_func);
            let indexed_us = t0.elapsed().as_micros();
            assert!(changed);
            assert_eq!(stats.index_builds, 1);
            assert_eq!(stats.rewrite_passes, 1);
            assert_eq!(stats.indexed_stmt_visits, 2 * n);

            let legacy_note = if n <= 256 {
                let mut legacy_func = base;
                let t1 = std::time::Instant::now();
                let (l_changed, stmt_scans, _) = legacy_apply_subvar_flow_pass(&mut legacy_func);
                let legacy_us = t1.elapsed().as_micros();
                assert_eq!(changed, l_changed);
                assert_eq!(
                    format!("{:?}", indexed_func.body),
                    format!("{:?}", legacy_func.body)
                );
                format!("legacy_us={legacy_us} legacy_stmt_scans={stmt_scans}")
            } else {
                "legacy=skipped".to_string()
            };
            eprintln!(
                "subvar_flow independent N={n}: indexed_us={indexed_us} stmt_visits={} builds={} rewrites={} | {legacy_note}",
                stats.indexed_stmt_visits, stats.index_builds, stats.rewrite_passes
            );
        }

        // 2. Doubling-size copy chains: v_1 = p0; v_2 = v_1; ...; y = v_L & 0xff; z = y == 1
        for len in [32usize, 64, 128, 256, 512] {
            let mut locals = Vec::with_capacity(len + 2);
            let mut body = Vec::with_capacity(len + 2);
            let mut prev = "p0".to_string();
            for i in 1..=len {
                let cur = format!("v_{i}");
                locals.push(b(&cur, NirBindingOrigin::Temp));
                body.push(PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(cur.clone()),
                    rhs: PreHirExpr::Var(prev),
                });
                prev = cur;
            }
            locals.push(b("y", NirBindingOrigin::Temp));
            locals.push(b("z", NirBindingOrigin::Temp));
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("y".into()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var(prev)),
                    rhs: Box::new(PreHirExpr::Const(0xff, u64_ty())),
                    ty: u64_ty(),
                },
            });
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("z".into()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(PreHirExpr::Var("y".into())),
                    rhs: Box::new(PreHirExpr::Const(1, u64_ty())),
                    ty: NirType::Bool,
                },
            });
            let base = PreHirFunction {
                name: format!("chain_{len}"),
                params: vec![b("p0", NirBindingOrigin::ParamIndex(0))],
                locals,
                body,
                ..Default::default()
            };

            let mut indexed_func = base.clone();
            let t0 = std::time::Instant::now();
            let (changed, stats) = apply_subvar_flow_pass_with_stats(&mut indexed_func);
            let indexed_us = t0.elapsed().as_micros();
            assert!(changed);
            assert_eq!(stats.index_builds, 1);
            assert_eq!(stats.rewrite_passes, 1);
            assert_eq!(stats.indexed_stmt_visits, len + 2);

            let mut legacy_func = base;
            let t1 = std::time::Instant::now();
            let (l_changed, stmt_scans, _) = legacy_apply_subvar_flow_pass(&mut legacy_func);
            let legacy_us = t1.elapsed().as_micros();
            assert_eq!(changed, l_changed);
            assert_eq!(
                format!("{:?}", indexed_func.body),
                format!("{:?}", legacy_func.body)
            );
            eprintln!(
                "subvar_flow chain L={len}: indexed_us={indexed_us} stmt_visits={} legacy_us={legacy_us} legacy_stmt_scans={stmt_scans}",
                stats.indexed_stmt_visits
            );
        }
    }
}
