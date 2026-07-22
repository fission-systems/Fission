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
    rhs: DirExpr,
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
    stmts: &[DirStmt],
    assigns: &mut Vec<AssignInfo>,
    multi_def: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
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
            DirStmt::Assign {
                lhs: DirLValue::Deref { ptr, .. },
                rhs,
            } => {
                collect_expr_assigns(ptr, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            DirStmt::Assign {
                lhs: DirLValue::Index { base, index, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(index, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            DirStmt::Assign {
                lhs: DirLValue::FieldAccess { base, .. },
                rhs,
            } => {
                collect_expr_assigns(base, assigns, multi_def);
                collect_expr_assigns(rhs, assigns, multi_def);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                collect_expr_assigns(expr, assigns, multi_def);
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_expr_assigns(va_list, assigns, multi_def);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns, multi_def);
            }
            DirStmt::For {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_assigns(cond, assigns, multi_def);
                collect_assignments(then_body, assigns, multi_def);
                collect_assignments(else_body, assigns, multi_def);
            }
            DirStmt::Switch {
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
    _expr: &DirExpr,
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
        op: DirBinaryOp,
        other: DirExpr,
        dest: String,
    },
    Compare {
        op: DirBinaryOp,
        other: DirExpr,
    },
    ShiftAmount {
        op: DirBinaryOp,
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
fn expr_contains_var(expr: &DirExpr, var_name: &str) -> bool {
    match expr {
        DirExpr::Var(name) => name == var_name,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, var_name),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, var_name) || expr_contains_var(rhs, var_name)
        }
        DirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, var_name)),
        DirExpr::Index { base, index, .. } => {
            expr_contains_var(base, var_name) || expr_contains_var(index, var_name)
        }
        DirExpr::Select {
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
fn find_uses(stmts: &[DirStmt], var_name: &str, uses: &mut Vec<UseInfo>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(dest),
                rhs,
            } => {
                analyze_expr_use(rhs, var_name, Some(dest), uses);
            }
            DirStmt::Assign { lhs, rhs } => {
                analyze_lvalue_use(lhs, var_name, uses);
                analyze_expr_use(rhs, var_name, None, uses);
            }
            DirStmt::Expr(expr) => {
                analyze_expr_use(expr, var_name, None, uses);
            }
            DirStmt::Return(Some(expr)) => {
                if expr_contains_var(expr, var_name) {
                    if let DirExpr::Var(name) = expr {
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
            DirStmt::Return(None) => {}
            DirStmt::Block(body) => {
                find_uses(body, var_name, uses);
            }
            DirStmt::While { cond, body } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(body, var_name, uses);
            }
            DirStmt::DoWhile { body, cond } => {
                find_uses(body, var_name, uses);
                analyze_expr_use(cond, var_name, None, uses);
            }
            DirStmt::For {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                analyze_expr_use(cond, var_name, None, uses);
                find_uses(then_body, var_name, uses);
                find_uses(else_body, var_name, uses);
            }
            DirStmt::Switch {
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

fn analyze_expr_use(expr: &DirExpr, var_name: &str, dest: Option<&str>, uses: &mut Vec<UseInfo>) {
    if !expr_contains_var(expr, var_name) {
        return;
    }
    match expr {
        DirExpr::Var(name) => {
            if name == var_name {
                if let Some(d) = dest {
                    uses.push(UseInfo {
                        context: UseContext::Binary {
                            op: DirBinaryOp::And,
                            other: DirExpr::Const(-1, NirType::Unknown),
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
        DirExpr::Binary { op, lhs, rhs, .. } => {
            let left_var = match &**lhs {
                DirExpr::Var(name) => name == var_name,
                _ => false,
            };
            let right_var = match &**rhs {
                DirExpr::Var(name) => name == var_name,
                _ => false,
            };

            if left_var || right_var {
                let other = if left_var { &**rhs } else { &**lhs };
                match op {
                    DirBinaryOp::Eq
                    | DirBinaryOp::Ne
                    | DirBinaryOp::Lt
                    | DirBinaryOp::Le
                    | DirBinaryOp::Gt
                    | DirBinaryOp::Ge
                    | DirBinaryOp::SLt
                    | DirBinaryOp::SLe
                    | DirBinaryOp::SGt
                    | DirBinaryOp::SGe => {
                        uses.push(UseInfo {
                            context: UseContext::Compare {
                                op: *op,
                                other: other.clone(),
                            },
                        });
                    }
                    DirBinaryOp::And => {
                        if let (Some(d), DirExpr::Const(mask, _)) = (dest, other) {
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
                    DirBinaryOp::Add
                    | DirBinaryOp::Sub
                    | DirBinaryOp::Or
                    | DirBinaryOp::Xor
                    | DirBinaryOp::Shl
                    | DirBinaryOp::Shr
                    | DirBinaryOp::Sar => {
                        if let Some(d) = dest {
                            if (*op == DirBinaryOp::Shl
                                || *op == DirBinaryOp::Shr
                                || *op == DirBinaryOp::Sar)
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
        DirExpr::Cast { ty, expr } => {
            if let DirExpr::Var(name) = &**expr {
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
        DirExpr::Call { target, args, .. } => {
            for (idx, arg) in args.iter().enumerate() {
                if let DirExpr::Var(name) = arg {
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
fn trace_or_subvar_piece(lhs: &DirExpr, rhs: &DirExpr, mask: u64) -> Option<(String, u64)> {
    let (low_part, high_part) = match (lhs, rhs) {
        (
            low,
            DirExpr::Binary {
                op: DirBinaryOp::Shl,
                ..
            },
        ) => (low, rhs),
        (
            DirExpr::Binary {
                op: DirBinaryOp::Shl,
                ..
            },
            low,
        ) => (low, lhs),
        _ => return None,
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs: shifted,
        rhs: shift_amt,
        ..
    } = high_part
    else {
        return None;
    };
    let DirExpr::Const(n, _) = shift_amt.as_ref() else {
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
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => match (and_lhs.as_ref(), and_rhs.as_ref()) {
            (DirExpr::Var(name), DirExpr::Const(m, _)) => (name.clone(), *m as u64),
            (DirExpr::Const(m, _), DirExpr::Var(name)) => (name.clone(), *m as u64),
            _ => return None,
        },
        _ => return None,
    };
    if and_mask != expected_low_mask {
        return None;
    }
    let DirExpr::Binary {
        op: DirBinaryOp::Shr | DirBinaryOp::Sar,
        lhs: shr_lhs,
        rhs: shr_amt,
        ..
    } = shifted.as_ref()
    else {
        return None;
    };
    let DirExpr::Const(shr_n, _) = shr_amt.as_ref() else {
        return None;
    };
    if shr_n != n {
        return None;
    }
    match shr_lhs.as_ref() {
        DirExpr::Var(name) if name == &src => Some((src, mask)),
        _ => None,
    }
}

fn analyze_lvalue_use(lhs: &DirLValue, var_name: &str, uses: &mut Vec<UseInfo>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => {
            if expr_contains_var(ptr, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        DirLValue::Index { base, index, .. } => {
            if expr_contains_var(base, var_name) || expr_contains_var(index, var_name) {
                uses.push(UseInfo {
                    context: UseContext::Incompatible,
                });
            }
        }
        DirLValue::FieldAccess { base, .. } => {
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
    def_map: HashMap<String, DirExpr>,
    type_map: HashMap<String, NirType>,
    multi_defined: HashSet<String>,
    varmap: HashMap<String, ReplaceVar>,
    pull_points: HashSet<String>,
    worklist: Vec<(String, u64)>,
    pull_count: usize,
}

impl SubvarFlowSolver {
    fn solve(&mut self, body: &[DirStmt]) -> bool {
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
            // deliberately-unregistered `DirExpr::Var`, as in the Windows
            // TEB/PEB field recognition in `fission-pcode`).
            None => return self.type_map.contains_key(var_name),
        };

        match def_expr {
            DirExpr::Binary { op, lhs, rhs, .. } => match op {
                DirBinaryOp::Or => {
                    if let Some((src, piece_mask)) = trace_or_subvar_piece(lhs, rhs, mask) {
                        self.worklist.push((src, piece_mask));
                        return true;
                    }
                    if let DirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let DirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::And | DirBinaryOp::Xor => {
                    if let DirExpr::Var(l) = &**lhs {
                        self.worklist.push((l.clone(), mask));
                    }
                    if let DirExpr::Var(r) = &**rhs {
                        self.worklist.push((r.clone(), mask));
                    }
                    true
                }
                DirBinaryOp::Shl => {
                    if let DirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask >> sa;
                            if new_mask == 0 {
                                true
                            } else if (new_mask << sa) == mask {
                                if let DirExpr::Var(l) = &**lhs {
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
                DirBinaryOp::Shr | DirBinaryOp::Sar => {
                    if let DirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa < 64 {
                            let new_mask = mask << sa;
                            if (new_mask >> sa) == mask {
                                if let DirExpr::Var(l) = &**lhs {
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
            DirExpr::Cast { expr, .. } => {
                if let DirExpr::Var(inner) = &**expr {
                    self.worklist.push((inner.clone(), mask));
                    true
                } else {
                    false
                }
            }
            DirExpr::Unary { op, expr, .. } => {
                if *op == DirUnaryOp::BitNot || *op == DirUnaryOp::Neg {
                    if let DirExpr::Var(inner) = &**expr {
                        self.worklist.push((inner.clone(), mask));
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            DirExpr::Var(src) => {
                self.worklist.push((src.clone(), mask));
                true
            }
            DirExpr::Const(_, _) => true,
            DirExpr::Load { .. } | DirExpr::Call { .. } => true,
            _ => false,
        }
    }

    fn trace_forward(&mut self, body: &[DirStmt], var_name: &str, mask: u64) -> bool {
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
                    DirBinaryOp::Add
                    | DirBinaryOp::Sub
                    | DirBinaryOp::And
                    | DirBinaryOp::Or
                    | DirBinaryOp::Xor => {
                        self.worklist.push((dest.clone(), mask));
                        if let DirExpr::Var(o) = other {
                            self.worklist.push((o.clone(), mask));
                        }
                    }
                    _ => return false,
                },
                UseContext::Compare { op: _, other } => {
                    match other {
                        DirExpr::Const(val, _) => {
                            if (val as u64 & !mask) != 0 {
                                return false;
                            }
                        }
                        DirExpr::Var(o) => {
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
fn rewrite_expr(expr: &mut DirExpr, varmap: &HashMap<String, ReplaceVar>) {
    match expr {
        DirExpr::Cast { ty, expr: inner } => {
            if let DirExpr::Var(name) = &**inner {
                if let Some(rep) = varmap.get(name) {
                    if let NirType::Int { bits, .. } = ty {
                        if *bits == rep.bitsize {
                            *expr = DirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }
            rewrite_expr(inner, varmap);
        }
        DirExpr::Binary { op, lhs, rhs, ty } => {
            if *op == DirBinaryOp::And {
                if let (DirExpr::Var(name), DirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = DirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
                if let (DirExpr::Const(mask, _), DirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if let Some(rep) = varmap.get(name) {
                        if *mask as u64 == rep.mask {
                            *expr = DirExpr::Var(rep.new_name.clone());
                            return;
                        }
                    }
                }
            }

            let l_narrow_ty = if let DirExpr::Var(n) = &**lhs {
                varmap.get(n).map(|r| r.new_type.clone())
            } else {
                None
            };
            let r_narrow_ty = if let DirExpr::Var(n) = &**rhs {
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
        DirExpr::Unary {
            expr: inner, ty, ..
        } => {
            let narrow_ty = if let DirExpr::Var(name) = &**inner {
                varmap.get(name).map(|rep| rep.new_type.clone())
            } else {
                None
            };
            rewrite_expr(inner, varmap);
            if let Some(nty) = narrow_ty {
                *ty = nty;
            }
        }
        DirExpr::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        _ => {
            for_each_child_expr(expr, |child| rewrite_expr(child, varmap));
        }
    }
}

fn for_each_child_expr<F>(expr: &mut DirExpr, mut f: F)
where
    F: FnMut(&mut DirExpr),
{
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            f(inner);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                f(arg);
            }
        }
        DirExpr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        DirExpr::Select {
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

fn rewrite_lvalue(lhs: &mut DirLValue, varmap: &HashMap<String, ReplaceVar>) {
    match lhs {
        DirLValue::Var(name) => {
            if let Some(rep) = varmap.get(name) {
                *name = rep.new_name.clone();
            }
        }
        DirLValue::Deref { ptr, .. } => {
            rewrite_expr(ptr, varmap);
        }
        DirLValue::Index { base, index, .. } => {
            rewrite_expr(base, varmap);
            rewrite_expr(index, varmap);
        }
        DirLValue::FieldAccess { base, .. } => {
            rewrite_expr(base, varmap);
        }
    }
}

fn rewrite_stmt(stmt: &mut DirStmt, varmap: &HashMap<String, ReplaceVar>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            rewrite_lvalue(lhs, varmap);
            rewrite_expr(rhs, varmap);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            rewrite_expr(expr, varmap);
        }
        DirStmt::VaStart { va_list, .. } => {
            rewrite_expr(va_list, varmap);
        }
        DirStmt::Block(body) => {
            rewrite_stmts(body, varmap);
        }
        DirStmt::While { cond, body } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(body, varmap);
        }
        DirStmt::DoWhile { body, cond } => {
            rewrite_stmts(body, varmap);
            rewrite_expr(cond, varmap);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_expr(cond, varmap);
            rewrite_stmts(then_body, varmap);
            rewrite_stmts(else_body, varmap);
        }
        DirStmt::Switch {
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

fn rewrite_stmts(stmts: &mut [DirStmt], varmap: &HashMap<String, ReplaceVar>) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(stmt, varmap);
    }
}

/// Pipeline entry point for the Global Subvariable Flow Analyzer normalization pass.
pub fn apply_subvar_flow_pass(func: &mut DirFunction) -> bool {
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
            DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                if let (DirExpr::Var(name), DirExpr::Const(mask, _)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
                if let (DirExpr::Const(mask, _), DirExpr::Var(name)) = (&**lhs, &**rhs) {
                    if is_valid_subvar_mask(*mask as u64) {
                        candidate_set.insert((name.clone(), *mask as u64));
                    }
                }
            }
            DirExpr::Cast { ty, expr } => {
                if let DirExpr::Var(name) = &**expr {
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
                    let binding = DirBinding {
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
        let mut func = DirFunction::default();
        func.name = "test_subflow".to_string();

        // `a`/`b` are function parameters (real, declared storage feeding
        // `x = a + b`) -- must be registered like any genuine binding, or
        // the def-less leaf case in `trace_backward` now conservatively
        // refuses to narrow them (an unregistered name reaching that path
        // is treated as a synthetic value with no real storage to declare,
        // not a parameter -- see the Windows TEB/PEB field regression this
        // guarded against).
        func.params.push(DirBinding {
            name: "a".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        });
        func.params.push(DirBinding {
            name: "b".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(1)),
            initializer: None,
        });
        func.locals.push(DirBinding {
            name: "x".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(DirBinding {
            name: "y".to_string(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        func.locals.push(DirBinding {
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
        let stmt1 = DirStmt::Assign {
            lhs: DirLValue::Var("x".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("a".to_string())),
                rhs: Box::new(DirExpr::Var("b".to_string())),
                ty: u64_ty(),
            },
        };
        let stmt2 = DirStmt::Assign {
            lhs: DirLValue::Var("y".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs: Box::new(DirExpr::Var("x".to_string())),
                rhs: Box::new(DirExpr::Const(0xff, u64_ty())),
                ty: u64_ty(),
            },
        };
        let stmt3 = DirStmt::Assign {
            lhs: DirLValue::Var("z".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Eq,
                lhs: Box::new(DirExpr::Var("y".to_string())),
                rhs: Box::new(DirExpr::Const(12, u64_ty())),
                ty: NirType::Bool,
            },
        };

        func.body = vec![stmt1, stmt2, stmt3];

        let changed = apply_subvar_flow_pass(&mut func);
        assert!(changed);

        // Verify z comparison is bridged and y's masking AND is completely eliminated
        if let DirStmt::Assign { rhs, .. } = &func.body[2] {
            if let DirExpr::Binary { lhs, rhs, .. } = rhs {
                if let DirExpr::Var(name) = &**lhs {
                    assert_eq!(name, "x_sub8");
                } else {
                    panic!("LHS should be narrow variable x_sub8");
                }
                if let DirExpr::Const(val, _) = &**rhs {
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
