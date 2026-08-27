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

/// Recursively scans statement trees to collect all local assignments and track multi-defined variables.
fn collect_assignments(
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, .. },
                rhs,
            } => {
                collect_expr_assigns(ptr, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            PreHirStmt::Assign {
                lhs: PreHirLValue::Index { base, index, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(index, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            PreHirStmt::Assign {
                lhs: PreHirLValue::FieldAccess { base, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_expr_assigns(expr, assigns, multi_def);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_expr_assigns(va_list, assigns, multi_def);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns, multi_def);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    collect_assignments(std::slice::from_ref(i.as_ref()), assigns, multi_def);
                }
                if let Some(c) = cond {
                    collect_expr_assigns(c, assigns, multi_def);
                }
                if let Some(u) = update {
                    collect_assignments(std::slice::from_ref(u.as_ref()), assigns, multi_def);
                }
                collect_assignments(body, assigns, multi_def);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_assigns(cond, assigns, multi_def);
                collect_assignments(then_body, assigns, multi_def);
                collect_assignments(else_body, assigns, multi_def);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_expr_assigns(expr, assigns, multi_def);
                for case in cases {
                    collect_assignments(&case.body, assigns, multi_def);
                }
                collect_assignments(default, assigns, multi_def);
            }
            _ => {}
        }
    }
}

fn collect_expr_assigns(
    _expr: &PreHirExpr,
    _assigns: &mut Vec<AssignInfo>,
    _multi_def: &mut HashSet<String>,
) {
    // Inner expressions do not contain assignment statements in Fission's HIR.
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

/// Determines if an expression contains references to a variable.
fn expr_contains_var(expr: &PreHirExpr, var_name: &str) -> bool {
    match expr {
        PreHirExpr::Var(name) => name == var_name,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, var_name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, var_name) || expr_contains_var(rhs, var_name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, var_name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_var(base, var_name) || expr_contains_var(index, var_name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, var_name)
                || expr_contains_var(then_expr, var_name)
                || expr_contains_var(else_expr, var_name)
        }
        _ => false,
    }
}

/// Scans statement body recursively to find all usages and their contextual patterns for a given variable.
fn find_uses(stmts: &[PreHirStmt], var_name: &str, uses: &mut Vec<UseInfo>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(dest),
                rhs,
            } => {
                analyze_expr_use(rhs, var_name, Some(dest), uses);
            }
            PreHirStmt::Assign { lhs, rhs } => {
                analyze_lvalue_use(lhs, var_name, uses);
                analyze_expr_use(rhs, var_name, None, uses);
            }
            PreHirStmt::Expr(expr) => {
                analyze_expr_use(expr, var_name, None, uses);
            }
            PreHirStmt::Return(Some(expr)) => {
                if expr_contains_var(expr, var_name) {
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
            PreHirStmt::Return(None) => {}
            PreHirStmt::Block(body) => {
                find_uses(body, var_name, uses);
            }
            PreHirStmt::While { cond, body } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(body, var_name, uses);
            }
            PreHirStmt::DoWhile { body, cond } => {
                find_uses(body, var_name, uses);
                analyze_expr_use(cond, var_name, None, uses);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    find_uses(std::slice::from_ref(i.as_ref()), var_name, uses);
                }
                if let Some(c) = cond {
                    analyze_expr_use(c, var_name, None, uses);
                }
                if let Some(u) = update {
                    find_uses(std::slice::from_ref(u.as_ref()), var_name, uses);
                }
                find_uses(body, var_name, uses);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(then_body, var_name, uses);
                find_uses(else_body, var_name, uses);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                analyze_expr_use(expr, var_name, None, uses);
                for case in cases {
                    find_uses(&case.body, var_name, uses);
                }
                find_uses(default, var_name, uses);
            }
            _ => {}
        }
    }
}

fn analyze_expr_use(
    expr: &PreHirExpr,
    var_name: &str,
    dest: Option<&str>,
    uses: &mut Vec<UseInfo>,
) {
    if !expr_contains_var(expr, var_name) {
        return;
    }
    match expr {
        PreHirExpr::Var(name) => {
            if name == var_name {
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
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            let left_var = match &**lhs {
                PreHirExpr::Var(name) => name == var_name,
                _ => false,
            };
            let right_var = match &**rhs {
                PreHirExpr::Var(name) => name == var_name,
                _ => false,
            };

            if left_var || right_var {
                let other = if left_var { &**rhs } else { &**lhs };
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
                        uses.push(UseInfo {
                            context: UseContext::Compare {
                                op: *op,
                                other: other.clone(),
                            },
                        });
                    }
                    PreHirBinaryOp::And => {
                        if let (Some(d), PreHirExpr::Const(mask, _)) = (dest, other) {
                            uses.push(UseInfo {
                                context: UseContext::AndMask {
                                    mask: *mask as u64,
                                    dest: d.to_string(),
                                },
                            });
                        } else if let Some(d) = dest {
                            uses.push(UseInfo {
                                context: UseContext::Binary {
                                    op: *op,
                                    other: other.clone(),
                                    dest: d.to_string(),
                                },
                            });
                        } else {
                            uses.push(UseInfo {
                                context: UseContext::Incompatible,
                            });
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
                            if (*op == PreHirBinaryOp::Shl
                                || *op == PreHirBinaryOp::Shr
                                || *op == PreHirBinaryOp::Sar)
                                && right_var
                            {
                                uses.push(UseInfo {
                                    context: UseContext::ShiftAmount {
                                        op: *op,
                                        dest: d.to_string(),
                                    },
                                });
                            } else {
                                uses.push(UseInfo {
                                    context: UseContext::Binary {
                                        op: *op,
                                        other: other.clone(),
                                        dest: d.to_string(),
                                    },
                                });
                            }
                        } else {
                            uses.push(UseInfo {
                                context: UseContext::Incompatible,
                            });
                        }
                    }
                    _ => {
                        uses.push(UseInfo {
                            context: UseContext::Incompatible,
                        });
                    }
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
                if expr_contains_var(arg, var_name) {
                    uses.push(UseInfo {
                        context: UseContext::Incompatible,
                    });
                }
            }
        }
        _ => {
            uses.push(UseInfo {
                context: UseContext::Incompatible,
            });
        }
    }
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

fn analyze_lvalue_use(lhs: &PreHirLValue, var_name: &str, uses: &mut Vec<UseInfo>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => {
            if expr_contains_var(ptr, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        PreHirLValue::Index { base, index, .. } => {
            if expr_contains_var(base, var_name) || expr_contains_var(index, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        PreHirLValue::FieldAccess { base, .. } => {
            if expr_contains_var(base, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
    }
}

/// Global Subvariable Flow solver executing worklist bit constraint propagation backward and forward.
struct SubvarFlowSolver {
    def_map: HashMap<String, PreHirExpr>,
    type_map: HashMap<String, NirType>,
    multi_defined: HashSet<String>,
    varmap: HashMap<String, ReplaceVar>,
    pull_points: HashSet<String>,
    worklist: Vec<(String, u64)>,
    pull_count: usize,
}

impl SubvarFlowSolver {
    fn solve(&mut self, body: &[PreHirStmt]) -> bool {
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

            if !self.trace_forward(body, &var_name, mask) {
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
                PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::And
                | PreHirBinaryOp::Xor => {
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
                if *op == PreHirUnaryOp::BitNot || *op == PreHirUnaryOp::Neg {
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

    fn trace_forward(&mut self, body: &[PreHirStmt], var_name: &str, mask: u64) -> bool {
        let mut uses = Vec::new();
        find_uses(body, var_name, &mut uses);

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
                    PreHirBinaryOp::Add
                    | PreHirBinaryOp::Sub
                    | PreHirBinaryOp::And
                    | PreHirBinaryOp::Or
                    | PreHirBinaryOp::Xor => {
                        self.worklist.push((dest.clone(), mask));
                        if let PreHirExpr::Var(o) = other {
                            self.worklist.push((o.clone(), mask));
                        }
                    }
                    _ => return false,
                },
                UseContext::Compare { op: _, other } => {
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

/// Pipeline entry point for the Global Subvariable Flow Analyzer normalization pass.
pub fn apply_subvar_flow_pass(func: &mut PreHirFunction) -> bool {
    let mut assigns = Vec::new();
    let mut multi_defined = HashSet::default();
    collect_assignments(&func.body, &mut assigns, &mut multi_defined);

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

    let candidates: Vec<(String, u64)> = candidate_set.into_iter().collect();

    if candidates.is_empty() {
        return false;
    }

    let mut def_map = HashMap::default();
    for assign in assigns {
        if !multi_defined.contains(&assign.lhs) {
            def_map.insert(assign.lhs, assign.rhs);
        }
    }

    let mut changed = false;
    for (var_name, mask) in candidates {
        let mut solver = SubvarFlowSolver {
            def_map: def_map.clone(),
            type_map: type_map.clone(),
            multi_defined: multi_defined.clone(),
            varmap: HashMap::default(),
            pull_points: HashSet::default(),
            worklist: vec![(var_name.clone(), mask)],
            pull_count: 0,
        };

        if solver.solve(&func.body) {
            for replace_var in solver.varmap.values() {
                if !func.locals.iter().any(|l| l.name == replace_var.new_name) {
                    let binding = PreHirBinding {
                        name: replace_var.new_name.clone(),
                        ty: replace_var.new_type.clone(),
                        surface_type_name: None,
                        origin: Some(NirBindingOrigin::Temp),
                        initializer: None,
                    };
                    func.locals.push(binding);
                }
            }

            rewrite_stmts(&mut func.body, &solver.varmap);
            changed = true;
        }
    }
    changed
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
}
