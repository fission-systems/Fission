use super::super::*;
use std::collections::{HashMap, HashSet};

/// Identify if a name is a candidate trash register or compiler temporary.
/// This includes x86 caller-saved registers and synthetic registers (e.g. uVarX, xVarX),
/// but explicitly excludes promoted parameters (e.g. param_X).
fn is_trash_register_candidate(name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    let is_reg = matches!(
        name_lower.as_str(),
        "rax" | "eax" | "ax" | "al" | "ah" |
        "rcx" | "ecx" | "cx" | "cl" | "ch" |
        "rdx" | "edx" | "dx" | "dl" | "dh" |
        "rsi" | "esi" | "si" | "sil" |
        "rdi" | "edi" | "di" | "dil" |
        "r8" | "r8d" | "r8w" | "r8b" |
        "r9" | "r9d" | "r9w" | "r9b" |
        "r10" | "r10d" | "r10w" | "r10b" |
        "r11" | "r11d" | "r11w" | "r11b"
    ) || name_lower.starts_with("xmm")
      || name_lower.starts_with("ymm")
      || name_lower.starts_with("st");

    let is_temp = name.starts_with("uVar") || name.starts_with("xVar");

    (is_reg || is_temp) && !name.starts_with("param_")
}

fn type_bits(ty: &NirType) -> u32 {
    match ty {
        NirType::Int { bits, .. } => *bits,
        NirType::Float { bits, .. } => *bits,
        NirType::Ptr(_) => 64,
        NirType::Bool => 1,
        _ => 32,
    }
}

/// A topmost significant byte mask check.
/// Returns true if the constant mask retains only the topmost significant bytes.
fn is_topmost_byte_mask(val: i64, bits: u32) -> bool {
    let bits = if bits == 0 { 32 } else { bits };
    let mask = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let val_u = val as u64 & mask;

    for shift in [8, 16, 24, 32, 40, 48, 56] {
        if shift < bits {
            if val_u == ((mask << shift) & mask) {
                return true;
            }
        }
    }
    false
}

/// Recursively collect variable names read in an expression.
fn collect_vars_in_expr(expr: &HirExpr, vars: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            collect_vars_in_expr(inner, vars);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            collect_vars_in_expr(cond, vars);
            collect_vars_in_expr(then_expr, vars);
            collect_vars_in_expr(else_expr, vars);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, vars);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(base, vars);
            collect_vars_in_expr(index, vars);
        }
        HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
    }
}

/// Gather initial real uses (observable reads) and assignment dependencies.
fn collect_real_uses_and_deps(
    stmts: &[HirStmt],
    alive: &mut HashSet<String>,
    dep: &mut HashMap<String, HashSet<String>>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                match lhs {
                    HirLValue::Var(y) => {
                        let mut rhs_vars = HashSet::new();
                        collect_vars_in_expr(rhs, &mut rhs_vars);
                        dep.entry(y.clone()).or_default().extend(rhs_vars);
                    }
                    HirLValue::Deref { ptr, .. } => {
                        collect_vars_in_expr(ptr, alive);
                        collect_vars_in_expr(rhs, alive);
                    }
                    HirLValue::Index { base, index, .. } => {
                        collect_vars_in_expr(base, alive);
                        collect_vars_in_expr(index, alive);
                        collect_vars_in_expr(rhs, alive);
                    }
                    HirLValue::FieldAccess { base, .. } => {
                        collect_vars_in_expr(base, alive);
                        collect_vars_in_expr(rhs, alive);
                    }
                }
            }
            HirStmt::Expr(expr) => {
                collect_vars_in_expr(expr, alive);
            }
            HirStmt::Return(expr_opt) => {
                if let Some(expr) = expr_opt {
                    collect_vars_in_expr(expr, alive);
                }
            }
            HirStmt::If { cond, then_body, else_body } => {
                collect_vars_in_expr(cond, alive);
                collect_real_uses_and_deps(then_body, alive, dep);
                collect_real_uses_and_deps(else_body, alive, dep);
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                collect_vars_in_expr(cond, alive);
                collect_real_uses_and_deps(body, alive, dep);
            }
            HirStmt::For { init, cond, update, body } => {
                if let Some(i) = init {
                    collect_real_uses_and_deps(std::slice::from_ref(i.as_ref()), alive, dep);
                }
                if let Some(c) = cond {
                    collect_vars_in_expr(c, alive);
                }
                if let Some(u) = update {
                    collect_real_uses_and_deps(std::slice::from_ref(u.as_ref()), alive, dep);
                }
                collect_real_uses_and_deps(body, alive, dep);
            }
            HirStmt::Switch { expr, cases, default } => {
                collect_vars_in_expr(expr, alive);
                for case in cases {
                    collect_real_uses_and_deps(&case.body, alive, dep);
                }
                collect_real_uses_and_deps(default, alive, dep);
            }
            HirStmt::Block(body) => {
                collect_real_uses_and_deps(body, alive, dep);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_vars_in_expr(va_list, alive);
            }
            HirStmt::Break | HirStmt::Continue | HirStmt::Label(_) | HirStmt::Goto(_) => {}
        }
    }
}

/// Extract source variable name if the expression is propagating.
fn get_propagating_var_source(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name.as_str()),
        HirExpr::Cast { expr: inner, .. } => get_propagating_var_source(inner),
        HirExpr::Binary { op: HirBinaryOp::And, lhs, rhs, ty } => {
            let bits = type_bits(ty);
            match (lhs.as_ref(), rhs.as_ref()) {
                (HirExpr::Var(name), HirExpr::Const(val, _)) if is_topmost_byte_mask(*val, bits) => {
                    Some(name.as_str())
                }
                (HirExpr::Const(val, _), HirExpr::Var(name)) if is_topmost_byte_mask(*val, bits) => {
                    Some(name.as_str())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Traces derived variables for the set `t`.
fn collect_derived_vars(stmts: &[HirStmt], t: &HashSet<String>, out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Var(y) = lhs {
                    if let Some(src) = get_propagating_var_source(rhs) {
                        if t.contains(src) {
                            out.push(y.clone());
                        }
                    }
                }
            }
            HirStmt::If { then_body, else_body, .. } => {
                collect_derived_vars(then_body, t, out);
                collect_derived_vars(else_body, t, out);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_derived_vars(body, t, out);
            }
            HirStmt::For { init, update, body, .. } => {
                if let Some(i) = init {
                    collect_derived_vars(std::slice::from_ref(i.as_ref()), t, out);
                }
                if let Some(u) = update {
                    collect_derived_vars(std::slice::from_ref(u.as_ref()), t, out);
                }
                collect_derived_vars(body, t, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_derived_vars(&case.body, t, out);
                }
                collect_derived_vars(default, t, out);
            }
            HirStmt::Block(body) => {
                collect_derived_vars(body, t, out);
            }
            _ => {}
        }
    }
}

/// Verify that every read of variables in `t` inside `stmt` is an allowed trash use.
fn check_statement_uses_allowed(
    stmt: &HirStmt,
    t: &HashSet<String>,
    alive: &HashSet<String>,
) -> bool {
    fn contains_t(expr: &HirExpr, t: &HashSet<String>) -> bool {
        let mut vars = HashSet::new();
        collect_vars_in_expr(expr, &mut vars);
        !vars.is_disjoint(t)
    }

    fn lvalue_contains_t(lval: &HirLValue, t: &HashSet<String>) -> bool {
        match lval {
            HirLValue::Var(name) => t.contains(name),
            HirLValue::Deref { ptr, .. } => contains_t(ptr, t),
            HirLValue::Index { base, index, .. } => contains_t(base, t) || contains_t(index, t),
            HirLValue::FieldAccess { base, .. } => contains_t(base, t),
        }
    }

    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            match lhs {
                HirLValue::Var(y) => {
                    if contains_t(rhs, t) {
                        if t.contains(y) || !alive.contains(y) {
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                }
                _ => {
                    if lvalue_contains_t(lhs, t) || contains_t(rhs, t) {
                        false
                    } else {
                        true
                    }
                }
            }
        }
        HirStmt::Expr(expr) => !contains_t(expr, t),
        HirStmt::Return(expr_opt) => {
            if let Some(expr) = expr_opt {
                !contains_t(expr, t)
            } else {
                true
            }
        }
        HirStmt::If { cond, then_body, else_body } => {
            if contains_t(cond, t) {
                return false;
            }
            then_body.iter().all(|s| check_statement_uses_allowed(s, t, alive))
                && else_body.iter().all(|s| check_statement_uses_allowed(s, t, alive))
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            if contains_t(cond, t) {
                return false;
            }
            body.iter().all(|s| check_statement_uses_allowed(s, t, alive))
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                if !check_statement_uses_allowed(i, t, alive) {
                    return false;
                }
            }
            if let Some(c) = cond {
                if contains_t(c, t) {
                    return false;
                }
            }
            if let Some(u) = update {
                if !check_statement_uses_allowed(u, t, alive) {
                    return false;
                }
            }
            body.iter().all(|s| check_statement_uses_allowed(s, t, alive))
        }
        HirStmt::Switch { expr, cases, default } => {
            if contains_t(expr, t) {
                return false;
            }
            for case in cases {
                if !case.body.iter().all(|s| check_statement_uses_allowed(s, t, alive)) {
                    return false;
                }
            }
            default.iter().all(|s| check_statement_uses_allowed(s, t, alive))
        }
        HirStmt::Block(body) => {
            body.iter().all(|s| check_statement_uses_allowed(s, t, alive))
        }
        HirStmt::VaStart { va_list, .. } => !contains_t(va_list, t),
        HirStmt::Break | HirStmt::Continue | HirStmt::Label(_) | HirStmt::Goto(_) => true,
    }
}

/// Replace all reads of variables in `t` with `Const(0, ty)`.
fn replace_trash_uses(
    stmt: &mut HirStmt,
    t: &HashSet<String>,
    var_types: &HashMap<String, NirType>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_trash_in_expr(rhs, t, var_types, changed);
            match lhs {
                HirLValue::Var(_) => {}
                HirLValue::Deref { ptr, .. } => {
                    replace_trash_in_expr(ptr, t, var_types, changed);
                }
                HirLValue::Index { base, index, .. } => {
                    replace_trash_in_expr(base, t, var_types, changed);
                    replace_trash_in_expr(index, t, var_types, changed);
                }
                HirLValue::FieldAccess { base, .. } => {
                    replace_trash_in_expr(base, t, var_types, changed);
                }
            }
        }
        HirStmt::Expr(expr) => {
            replace_trash_in_expr(expr, t, var_types, changed);
        }
        HirStmt::Return(expr_opt) => {
            if let Some(expr) = expr_opt {
                replace_trash_in_expr(expr, t, var_types, changed);
            }
        }
        HirStmt::If { cond, then_body, else_body } => {
            replace_trash_in_expr(cond, t, var_types, changed);
            for s in then_body {
                replace_trash_uses(s, t, var_types, changed);
            }
            for s in else_body {
                replace_trash_uses(s, t, var_types, changed);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            replace_trash_in_expr(cond, t, var_types, changed);
            for s in body {
                replace_trash_uses(s, t, var_types, changed);
            }
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                replace_trash_uses(i, t, var_types, changed);
            }
            if let Some(c) = cond {
                replace_trash_in_expr(c, t, var_types, changed);
            }
            if let Some(u) = update {
                replace_trash_uses(u, t, var_types, changed);
            }
            for s in body {
                replace_trash_uses(s, t, var_types, changed);
            }
        }
        HirStmt::Switch { expr, cases, default } => {
            replace_trash_in_expr(expr, t, var_types, changed);
            for case in cases {
                for s in &mut case.body {
                    replace_trash_uses(s, t, var_types, changed);
                }
            }
            for s in default {
                replace_trash_uses(s, t, var_types, changed);
            }
        }
        HirStmt::Block(body) => {
            for s in body {
                replace_trash_uses(s, t, var_types, changed);
            }
        }
        HirStmt::VaStart { va_list, .. } => {
            replace_trash_in_expr(va_list, t, var_types, changed);
        }
        HirStmt::Break | HirStmt::Continue | HirStmt::Label(_) | HirStmt::Goto(_) => {}
    }
}

fn replace_trash_in_expr(
    expr: &mut HirExpr,
    t: &HashSet<String>,
    var_types: &HashMap<String, NirType>,
    changed: &mut bool,
) {
    match expr {
        HirExpr::Var(name) => {
            if t.contains(name) {
                let ty = var_types.get(name).cloned().unwrap_or(NirType::Unknown);
                *expr = HirExpr::Const(0, ty);
                *changed = true;
            }
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            replace_trash_in_expr(inner, t, var_types, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_trash_in_expr(lhs, t, var_types, changed);
            replace_trash_in_expr(rhs, t, var_types, changed);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            replace_trash_in_expr(cond, t, var_types, changed);
            replace_trash_in_expr(then_expr, t, var_types, changed);
            replace_trash_in_expr(else_expr, t, var_types, changed);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                replace_trash_in_expr(arg, t, var_types, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            replace_trash_in_expr(base, t, var_types, changed);
            replace_trash_in_expr(index, t, var_types, changed);
        }
        HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
    }
}

fn collect_all_reads(stmts: &[HirStmt], vars: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                collect_vars_in_expr(rhs, vars);
                match lhs {
                    HirLValue::Var(_) => {}
                    HirLValue::Deref { ptr, .. } => {
                        collect_vars_in_expr(ptr, vars);
                    }
                    HirLValue::Index { base, index, .. } => {
                        collect_vars_in_expr(base, vars);
                        collect_vars_in_expr(index, vars);
                    }
                    HirLValue::FieldAccess { base, .. } => {
                        collect_vars_in_expr(base, vars);
                    }
                }
            }
            HirStmt::Expr(expr) => {
                collect_vars_in_expr(expr, vars);
            }
            HirStmt::Return(expr_opt) => {
                if let Some(expr) = expr_opt {
                    collect_vars_in_expr(expr, vars);
                }
            }
            HirStmt::If { cond, then_body, else_body } => {
                collect_vars_in_expr(cond, vars);
                collect_all_reads(then_body, vars);
                collect_all_reads(else_body, vars);
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                collect_vars_in_expr(cond, vars);
                collect_all_reads(body, vars);
            }
            HirStmt::For { init, cond, update, body } => {
                if let Some(i) = init {
                    collect_all_reads(std::slice::from_ref(i.as_ref()), vars);
                }
                if let Some(c) = cond {
                    collect_vars_in_expr(c, vars);
                }
                if let Some(u) = update {
                    collect_all_reads(std::slice::from_ref(u.as_ref()), vars);
                }
                collect_all_reads(body, vars);
            }
            HirStmt::Switch { expr, cases, default } => {
                collect_vars_in_expr(expr, vars);
                for case in cases {
                    collect_all_reads(&case.body, vars);
                }
                collect_all_reads(default, vars);
            }
            HirStmt::Block(body) => {
                collect_all_reads(body, vars);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_vars_in_expr(va_list, vars);
            }
            HirStmt::Break | HirStmt::Continue | HirStmt::Label(_) | HirStmt::Goto(_) => {}
        }
    }
}

/// Apply the likely trash elimination pass.
/// Finds parameters or local variables that are register/compiler trash candidates
/// and only flow into dead assignments or mask operations. Truncates their data-flow.
pub(crate) fn apply_likely_trash_pass(func: &mut HirFunction) -> bool {
    let mut var_types = HashMap::new();
    for local in &func.locals {
        var_types.insert(local.name.clone(), local.ty.clone());
    }
    for param in &func.params {
        var_types.insert(param.name.clone(), param.ty.clone());
    }

    // 1. Identify candidate variables
    let mut candidates = Vec::new();
    for local in &func.locals {
        if is_trash_register_candidate(&local.name) {
            candidates.push(local.name.clone());
        }
    }
    for param in &func.params {
        if is_trash_register_candidate(&param.name) {
            candidates.push(param.name.clone());
        }
    }

    if candidates.is_empty() {
        return false;
    }

    // 2. Perform variable aliveness analysis
    let mut alive = HashSet::new();
    let mut dep = HashMap::new();
    collect_real_uses_and_deps(&func.body, &mut alive, &mut dep);

    // Propagate aliveness
    let mut worklist: Vec<String> = alive.iter().cloned().collect();
    while let Some(var) = worklist.pop() {
        if let Some(deps) = dep.get(&var) {
            for d in deps {
                if alive.insert(d.clone()) {
                    worklist.push(d.clone());
                }
            }
        }
    }

    // 3. For each candidate, trace forward to find derived variables `t`, and verify allowed uses
    let mut trash_variables = HashSet::new();
    for c in candidates {
        // Build `t`
        let mut t = HashSet::new();
        t.insert(c.clone());

        // Fixed point loop to expand `t`
        loop {
            let mut changed = false;
            let mut new_derived = Vec::new();
            collect_derived_vars(&func.body, &t, &mut new_derived);
            for y in new_derived {
                if t.insert(y) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Verify all uses of variables in `t` are allowed trash uses
        let all_allowed = func.body.iter().all(|s| check_statement_uses_allowed(s, &t, &alive));
        if all_allowed {
            trash_variables.extend(t);
        }
    }

    if trash_variables.is_empty() {
        return false;
    }

    // 4. Truncate trash flow by replacing reads of trash_variables with Const(0, ty)
    let mut changed = false;
    for stmt in &mut func.body {
        replace_trash_uses(stmt, &trash_variables, &var_types, &mut changed);
    }

    // 5. Remove unused trash parameters from func.params
    let mut used_vars = HashSet::new();
    collect_all_reads(&func.body, &mut used_vars);

    func.params.retain(|param| {
        if is_trash_register_candidate(&param.name) && !used_vars.contains(&param.name) {
            changed = true;
            false
        } else {
            true
        }
    });

    changed
}
