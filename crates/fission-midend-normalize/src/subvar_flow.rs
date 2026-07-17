use crate::prelude::*;
use std::collections::{HashMap, HashSet};

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
    rhs: HirExpr,
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
    stmts: &[HirStmt],
    assigns: &mut Vec<AssignInfo>,
    multi_def: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
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
            HirStmt::Assign {
                lhs: HirLValue::Deref { ptr, .. },
                rhs,
            } => {
                collect_expr_assigns(ptr, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            HirStmt::Assign {
                lhs: HirLValue::Index { base, index, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(index, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            HirStmt::Assign {
                lhs: HirLValue::FieldAccess { base, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_expr_assigns(expr, assigns, multi_def);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_expr_assigns(va_list, assigns, multi_def);
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns, multi_def);
            }
            HirStmt::For {
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
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_assigns(cond, assigns, multi_def);
                collect_assignments(then_body, assigns, multi_def);
                collect_assignments(else_body, assigns, multi_def);
            }
            HirStmt::Switch {
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
    _expr: &HirExpr,
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
        op: HirBinaryOp,
        other: HirExpr,
        dest: String,
    },
    Compare {
        op: HirBinaryOp,
        other: HirExpr,
    },
    ShiftAmount {
        op: HirBinaryOp,
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
fn expr_contains_var(expr: &HirExpr, var_name: &str) -> bool {
    match expr {
        HirExpr::Var(name) => name == var_name,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, var_name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, var_name) || expr_contains_var(rhs, var_name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, var_name)),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, var_name) || expr_contains_var(index, var_name)
        }
        HirExpr::Select {
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
fn find_uses(stmts: &[HirStmt], var_name: &str, uses: &mut Vec<UseInfo>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(dest),
                rhs,
            } => {
                analyze_expr_use(rhs, var_name, Some(dest), uses);
            }
            HirStmt::Assign { lhs, rhs } => {
                analyze_lvalue_use(lhs, var_name, uses);
                analyze_expr_use(rhs, var_name, None, uses);
            }
            HirStmt::Expr(expr) => {
                analyze_expr_use(expr, var_name, None, uses);
            }
            HirStmt::Return(Some(expr)) => {
                if expr_contains_var(expr, var_name) {
                    if let HirExpr::Var(name) = expr {
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
            HirStmt::Return(None) => {}
            HirStmt::Block(body) => {
                find_uses(body, var_name, uses);
            }
            HirStmt::While { cond, body } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(body, var_name, uses);
            }
            HirStmt::DoWhile { body, cond } => {
                find_uses(body, var_name, uses);
                analyze_expr_use(cond, var_name, None, uses);
            }
            HirStmt::For {
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
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(then_body, var_name, uses);
                find_uses(else_body, var_name, uses);
            }
            HirStmt::Switch {
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

fn analyze_expr_use(expr: &HirExpr, var_name: &str, dest: Option<&str>, uses: &mut Vec<UseInfo>) {
    if !expr_contains_var(expr, var_name) {
        return;
    }
    match expr {
        HirExpr::Var(name) => {
            if name == var_name {
                if let Some(d) = dest {
                    uses.push(UseInfo {
                        context: UseContext::Binary {
                            op: HirBinaryOp::And,
                            other: HirExpr::Const(-1, NirType::Unknown),
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
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let left_var = match &**lhs {
                HirExpr::Var(name) => name == var_name,
                _ => false,
            };
            let right_var = match &**rhs {
                HirExpr::Var(name) => name == var_name,
                _ => false,
            };

            if left_var || right_var {
                let other = if left_var { &**rhs } else { &**lhs };
                match op {
                    HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::Gt
                    | HirBinaryOp::Ge
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
                    | HirBinaryOp::SGt
                    | HirBinaryOp::SGe => {
                        uses.push(UseInfo {
                            context: UseContext::Compare {
                                op: *op,
                                other: other.clone(),
                            },
                        });
                    }
                    HirBinaryOp::And => {
                        if let (Some(d), HirExpr::Const(mask, _)) = (dest, other) {
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
                    HirBinaryOp::Add
                    | HirBinaryOp::Sub
                    | HirBinaryOp::Or
                    | HirBinaryOp::Xor
                    | HirBinaryOp::Shl
                    | HirBinaryOp::Shr
                    | HirBinaryOp::Sar => {
                        if let Some(d) = dest {
                            if (*op == HirBinaryOp::Shl
                                || *op == HirBinaryOp::Shr
                                || *op == HirBinaryOp::Sar)
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
        HirExpr::Cast { ty, expr } => {
            if let HirExpr::Var(name) = &**expr {
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
        HirExpr::Call { target, args, .. } => {
            for (idx, arg) in args.iter().enumerate() {
                if let HirExpr::Var(name) = arg {
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
fn trace_or_subvar_piece(lhs: &HirExpr, rhs: &HirExpr, mask: u64) -> Option<(String, u64)> {
    let (low_part, high_part) = match (lhs, rhs) {
        (
            low,
            HirExpr::Binary {
                op: HirBinaryOp::Shl,
                ..
            },
        ) => (low, rhs),
        (
            HirExpr::Binary {
                op: HirBinaryOp::Shl,
                ..
            },
            low,
        ) => (low, lhs),
        _ => return None,
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shifted,
        rhs: shift_amt,
        ..
    } = high_part
    else {
        return None;
    };
    let HirExpr::Const(n, _) = shift_amt.as_ref() else {
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
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => match (and_lhs.as_ref(), and_rhs.as_ref()) {
            (HirExpr::Var(name), HirExpr::Const(m, _)) => (name.clone(), *m as u64),
            (HirExpr::Const(m, _), HirExpr::Var(name)) => (name.clone(), *m as u64),
            _ => return None,
        },
        _ => return None,
    };
    if and_mask != expected_low_mask {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs: shr_lhs,
        rhs: shr_amt,
        ..
    } = shifted.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shr_n, _) = shr_amt.as_ref() else {
        return None;
    };
    if shr_n != n {
        return None;
    }
    match shr_lhs.as_ref() {
        HirExpr::Var(name) if name == &src => Some((src, mask)),
        _ => None,
    }
}

fn analyze_lvalue_use(lhs: &HirLValue, var_name: &str, uses: &mut Vec<UseInfo>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            if expr_contains_var(ptr, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        HirLValue::Index { base, index, .. } => {
            if expr_contains_var(base, var_name) || expr_contains_var(index, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        HirLValue::FieldAccess { base, .. } => {
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
    def_map: HashMap<String, HirExpr>,
    type_map: HashMap<String, NirType>,
    multi_defined: HashSet<String>,
    varmap: HashMap<String, ReplaceVar>,
    pull_points: HashSet<String>,
    worklist: Vec<(String, u64)>,
    pull_count: usize,
}

impl SubvarFlowSolver {
    fn solve(&mut self, body: &[HirStmt]) -> bool {
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
            None => return true, // Leaf parameter / input boundary
        };

        match def_expr {
            HirExpr::Binary { op, lhs, rhs, .. } => match op {
                HirBinaryOp::Or => {
                    if let Some((src, piece_mask)) = trace_or_subvar_piece(lhs, rhs, mask) {
                        self.worklist.push((src, piece_mask));
                        return true;
                    }
                    if let HirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let HirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                HirBinaryOp::Add | HirBinaryOp::Sub | HirBinaryOp::And | HirBinaryOp::Xor => {
                    if let HirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let HirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                HirBinaryOp::Shl => {
                    if let HirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask >> sa;
                            if new_mask == 0 {
                                true
                            } else if (new_mask << sa) == mask {
                                if let HirExpr::Var(l) = &**lhs {
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
                HirBinaryOp::Shr | HirBinaryOp::Sar => {
                    if let HirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask << sa;
                            if (new_mask >> sa) == mask {
                                if let HirExpr::Var(l) = &**lhs {
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
            HirExpr::Cast { expr, .. } => {
                if let HirExpr::Var(inner) = &**expr {
                    self.worklist.push((inner.clone(), mask));
                    true
                } else {
                    false
                }
            }
            HirExpr::Unary { op, expr, .. } => {
                if *op == HirUnaryOp::BitNot || *op == HirUnaryOp::Neg {
                    if let HirExpr::Var(inner) = &**expr {
                        self.worklist.push((inner.clone(), mask));
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            HirExpr::Var(src) => {
                self.worklist.push((src.clone(), mask));
                true
            }
            HirExpr::Const(_, _) => true,
            HirExpr::Load { .. } | HirExpr::Call { .. } => true,
            _ => false,
        }
    }

    fn trace_forward(&mut self, body: &[HirStmt], var_name: &str, mask: u64) -> bool {
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
                    HirBinaryOp::Add
                    | HirBinaryOp::Sub
                    | HirBinaryOp::And
                    | HirBinaryOp::Or
                    | HirBinaryOp::Xor => {
                        self.worklist.push((dest.clone(), mask));
                        if let HirExpr::Var(o) = other {
                            self.worklist.push((o.clone(), mask));
                        }
                    }
                    _ => return false,
                },
                UseContext::Compare { op: _, other } => {
                    match other {
                        HirExpr::Const(val, _) => {
                            if (val as u64 & !mask) != 0 {
                                return false;
                            }
                        }
                        HirExpr::Var(o) => {
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
fn rewrite_expr(expr: &mut HirExpr, varmap: &HashMap<String, ReplaceVar>) {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
            if let HirExpr::Var(name) = &**inner {
                if let Some(rep) = varmap.get(name) {
                    if let NirType::Int { bits, .. } = ty {
                        if *bits == rep.bitsize {
                            *expr = HirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }
            rewrite_expr(inner, varmap);
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            if *op == HirBinaryOp::And {
                if let (HirExpr::Var(name), HirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = HirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
                if let (HirExpr::Const(mask, _), HirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = HirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }

            let l_narrow_ty = if let HirExpr::Var(n) = &**lhs {
                varmap.get(n).map(|r| r.new_type.clone())
            } else {
                None
            };
            let r_narrow_ty = if let HirExpr::Var(n) = &**rhs {
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
        HirExpr::Unary {
            expr: inner, ty, ..
        } => {
            let narrow_ty = if let HirExpr::Var(name) = &**inner {
                varmap.get(name).map(|rep| rep.new_type.clone())
            } else {
                None
            };
            rewrite_expr(inner, varmap);
            if let Some(nty) = narrow_ty {
                *ty = nty;
            }
        }
        HirExpr::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        _ => {
            for_each_child_expr(expr, |child| rewrite_expr(child, varmap));
        }
    }
}

fn for_each_child_expr<F>(expr: &mut HirExpr, mut f: F)
where
    F: FnMut(&mut HirExpr),
{
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            f(inner);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                f(arg);
            }
        }
        HirExpr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        HirExpr::Select {
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

fn rewrite_lvalue(lhs: &mut HirLValue, varmap: &HashMap<String, ReplaceVar>) {
    match lhs {
        HirLValue::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        HirLValue::Deref { ptr, .. } => {
            rewrite_expr(ptr, varmap);
        }
        HirLValue::Index { base, index, .. } => {
            rewrite_expr(base, varmap);
            rewrite_expr(index, varmap);
        }
        HirLValue::FieldAccess { base, .. } => {
            rewrite_expr(base, varmap);
        }
    }
}

fn rewrite_stmt(stmt: &mut HirStmt, varmap: &HashMap<String, ReplaceVar>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            rewrite_lvalue(lhs, varmap);
            rewrite_expr(rhs, varmap);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            rewrite_expr(expr, varmap);
        }
        HirStmt::VaStart { va_list, .. } => {
            rewrite_expr(va_list, varmap);
        }
        HirStmt::Block(body) => {
            rewrite_stmts(body, varmap);
        }
        HirStmt::While { cond, body } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(body, varmap);
        }
        HirStmt::DoWhile { body, cond } => {
            rewrite_stmts(body, varmap);
            rewrite_expr(cond, varmap);
        }
        HirStmt::For {
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
            rewrite_stmts(body, varmap);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(then_body, varmap);
            rewrite_stmts(else_body, varmap);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_expr(expr, varmap);
            for case in cases {
                rewrite_stmts(&mut case.body, varmap);
            }
            rewrite_stmts(default, varmap);
        }
        _ => {}
    }
}

fn rewrite_stmts(stmts: &mut [HirStmt], varmap: &HashMap<String, ReplaceVar>) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(stmt, varmap);
    }
}

/// Pipeline entry point for the Global Subvariable Flow Analyzer normalization pass.
pub fn apply_subvar_flow_pass(func: &mut HirFunction) -> bool {
    let mut assigns = Vec::new();
    let mut multi_defined = HashSet::new();
    collect_assignments(&func.body, &mut assigns, &mut multi_defined);

    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let nz_masks = crate::global_opt::compute_nz_masks(func);
    let mut candidate_set = HashSet::new();

    for assign in &assigns {
        if multi_defined.contains(&assign.lhs) {
            continue;
        }
        match &assign.rhs {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                if let (HirExpr::Var(name), HirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
                if let (HirExpr::Const(mask, _), HirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
            }
            HirExpr::Cast { ty, expr } => {
                if let HirExpr::Var(name) = &**expr {
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

    let mut def_map = HashMap::new();
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
            varmap: HashMap::new(),
            pull_points: HashSet::new(),
            worklist: vec![(var_name.clone(), mask)],
            pull_count: 0,
        };

        if solver.solve(&func.body) {
            for replace_var in solver.varmap.values() {
                if !func.locals.iter().any(|l| l.name == replace_var.new_name) {
                    let binding = NirBinding {
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
        let mut func = HirFunction::default();
        func.name = "test_subflow".to_string();

        func.locals.push(NirBinding {
            name: "x".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(NirBinding {
            name: "y".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(NirBinding {
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
        let stmt1 = HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("a".to_string())),
                rhs: Box::new(HirExpr::Var("b".to_string())),
                ty: u64_ty(),
            },
        };
        let stmt2 = HirStmt::Assign {
            lhs: HirLValue::Var("y".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Var("x".to_string())),
                rhs: Box::new(HirExpr::Const(0xff, u64_ty())),
                ty: u64_ty(),
            },
        };
        let stmt3 = HirStmt::Assign {
            lhs: HirLValue::Var("z".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("y".to_string())),
                rhs: Box::new(HirExpr::Const(12, u64_ty())),
                ty: NirType::Bool,
            },
        };

        func.body = vec![stmt1, stmt2, stmt3];

        let changed = apply_subvar_flow_pass(&mut func);
        assert!(changed);

        // Verify z comparison is bridged and y's masking AND is completely eliminated
        if let HirStmt::Assign { rhs, .. } = &func.body[2] {
            if let HirExpr::Binary { lhs, rhs, .. } = rhs {
                if let HirExpr::Var(name) = &**lhs {
                    assert_eq!(name, "x_sub8");
                } else {
                    panic!("LHS should be narrow variable x_sub8");
                }
                if let HirExpr::Const(val, _) = &**rhs {
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
