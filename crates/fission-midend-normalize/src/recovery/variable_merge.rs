use crate::prelude::*; // For accessing normalizer structures
use fission_midend_dir::util::rename_vars_in_stmts;
use crate::{HashMap, HashSet};

fn collect_direct_copies(stmts: &[DirStmt]) -> crate::HashSet<(String, String)> {
    let mut copies = crate::HashSet::default();
    // Only *unconditional* var copies establish merge identity. A copy inside
    // `if`/`while`/switch (e.g. x86 cmov `if (cond) acc = src`) is a
    // path-sensitive override: merging `acc` with `src` collapses distinct
    // values (hi default vs lo source) and destroys clamp/min/max chains.
    fn visit(
        stmts: &[DirStmt],
        copies: &mut crate::HashSet<(String, String)>,
        under_control: bool,
    ) {
        for stmt in stmts {
            match stmt {
                DirStmt::Assign {
                    lhs: DirLValue::Var(lhs_name),
                    rhs: DirExpr::Var(rhs_name),
                } if !under_control => {
                    copies.insert((lhs_name.clone(), rhs_name.clone()));
                    copies.insert((rhs_name.clone(), lhs_name.clone()));
                }
                DirStmt::Block(body) => {
                    visit(body, copies, under_control);
                }
                DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                    visit(body, copies, true);
                }
                DirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    visit(then_body, copies, true);
                    visit(else_body, copies, true);
                }
                DirStmt::For {
                    init, update, body, ..
                } => {
                    // init runs once unconditionally; update/body are loop-controlled.
                    if let Some(init_stmt) = init {
                        visit(std::slice::from_ref(init_stmt), copies, under_control);
                    }
                    if let Some(update_stmt) = update {
                        visit(std::slice::from_ref(update_stmt), copies, true);
                    }
                    visit(body, copies, true);
                }
                DirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(&case.body, copies, true);
                    }
                    visit(default, copies, true);
                }
                _ => {}
            }
        }
    }
    visit(stmts, &mut copies, false);
    copies
}

fn collect_copy_merge_barrier_vars(
    stmts: &[DirStmt],
    stack_state_vars: HashSet<String>,
) -> HashSet<String> {
    let mut collector = CopyMergeBarrierCollector {
        stack_state_vars,
        ..Default::default()
    };
    collector.visit_stmts(stmts);
    collector.barrier_vars
}

fn collect_read_vars(stmts: &[DirStmt]) -> HashSet<String> {
    let mut vars = HashSet::default();
    collect_read_vars_in_stmts(stmts, &mut vars);
    vars
}

fn collect_cooccurring_var_pairs(stmts: &[DirStmt]) -> HashSet<(String, String)> {
    let mut pairs = HashSet::default();
    collect_cooccurring_var_pairs_in_stmts(stmts, &mut pairs);
    pairs
}

fn collect_cooccurring_var_pairs_in_stmts(
    stmts: &[DirStmt],
    pairs: &mut HashSet<(String, String)>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                collect_cooccurring_var_pairs_in_lvalue(lhs, pairs);
                collect_cooccurring_var_pairs_in_expr(rhs, pairs);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                collect_cooccurring_var_pairs_in_expr(expr, pairs);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_cooccurring_var_pairs_in_stmts(body, pairs);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_cooccurring_var_pairs_in_expr(cond, pairs);
                collect_cooccurring_var_pairs_in_stmts(then_body, pairs);
                collect_cooccurring_var_pairs_in_stmts(else_body, pairs);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_cooccurring_var_pairs_in_stmts(std::slice::from_ref(init), pairs);
                }
                if let Some(cond) = cond {
                    collect_cooccurring_var_pairs_in_expr(cond, pairs);
                }
                if let Some(update) = update {
                    collect_cooccurring_var_pairs_in_stmts(std::slice::from_ref(update), pairs);
                }
                collect_cooccurring_var_pairs_in_stmts(body, pairs);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_cooccurring_var_pairs_in_expr(expr, pairs);
                for case in cases {
                    collect_cooccurring_var_pairs_in_stmts(&case.body, pairs);
                }
                collect_cooccurring_var_pairs_in_stmts(default, pairs);
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_cooccurring_var_pairs_in_expr(va_list, pairs);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn collect_cooccurring_var_pairs_in_lvalue(
    lval: &DirLValue,
    pairs: &mut HashSet<(String, String)>,
) {
    match lval {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => collect_cooccurring_var_pairs_in_expr(ptr, pairs),
        DirLValue::Index { base, index, .. } => {
            collect_cooccurring_var_pairs_in_expr(base, pairs);
            collect_cooccurring_var_pairs_in_expr(index, pairs);
        }
        DirLValue::FieldAccess { base, .. } => collect_cooccurring_var_pairs_in_expr(base, pairs),
    }
}

fn collect_cooccurring_var_pairs_in_expr(expr: &DirExpr, pairs: &mut HashSet<(String, String)>) {
    let mut vars = HashSet::default();
    collect_vars_in_expr(expr, &mut vars);
    let mut vars = vars.into_iter().collect::<Vec<_>>();
    vars.sort();
    for i in 0..vars.len() {
        for j in (i + 1)..vars.len() {
            pairs.insert((vars[i].clone(), vars[j].clone()));
        }
    }

    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            collect_cooccurring_var_pairs_in_expr(inner, pairs);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_cooccurring_var_pairs_in_expr(lhs, pairs);
            collect_cooccurring_var_pairs_in_expr(rhs, pairs);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_cooccurring_var_pairs_in_expr(arg, pairs);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_cooccurring_var_pairs_in_expr(cond, pairs);
            collect_cooccurring_var_pairs_in_expr(then_expr, pairs);
            collect_cooccurring_var_pairs_in_expr(else_expr, pairs);
        }
        DirExpr::Index { base, index, .. } => {
            collect_cooccurring_var_pairs_in_expr(base, pairs);
            collect_cooccurring_var_pairs_in_expr(index, pairs);
        }
        DirExpr::Var(_) | DirExpr::Const(_, _) | DirExpr::AddressOfGlobal(_) => {}
    }
}

fn collect_read_vars_in_stmts(stmts: &[DirStmt], vars: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                collect_read_vars_in_lvalue(lhs, vars);
                collect_vars_in_expr(rhs, vars);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => collect_vars_in_expr(expr, vars),
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_read_vars_in_stmts(body, vars);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_vars_in_expr(cond, vars);
                collect_read_vars_in_stmts(then_body, vars);
                collect_read_vars_in_stmts(else_body, vars);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_read_vars_in_stmts(std::slice::from_ref(init), vars);
                }
                if let Some(cond) = cond {
                    collect_vars_in_expr(cond, vars);
                }
                if let Some(update) = update {
                    collect_read_vars_in_stmts(std::slice::from_ref(update), vars);
                }
                collect_read_vars_in_stmts(body, vars);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_vars_in_expr(expr, vars);
                for case in cases {
                    collect_read_vars_in_stmts(&case.body, vars);
                }
                collect_read_vars_in_stmts(default, vars);
            }
            DirStmt::VaStart { va_list, .. } => collect_vars_in_expr(va_list, vars),
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn collect_read_vars_in_lvalue(lval: &DirLValue, vars: &mut HashSet<String>) {
    match lval {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => collect_vars_in_expr(ptr, vars),
        DirLValue::Index { base, index, .. } => {
            collect_vars_in_expr(base, vars);
            collect_vars_in_expr(index, vars);
        }
        DirLValue::FieldAccess { base, .. } => collect_vars_in_expr(base, vars),
    }
}

#[derive(Default)]
struct CopyMergeBarrierCollector {
    barrier_vars: HashSet<String>,
    stack_state_vars: HashSet<String>,
}

impl CopyMergeBarrierCollector {
    fn visit_stmts(&mut self, stmts: &[DirStmt]) {
        for stmt in stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &DirStmt) {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                if let DirLValue::Var(lhs_name) = lhs
                    && self.expr_is_load_derived_barrier(rhs)
                {
                    self.barrier_vars.insert(lhs_name.clone());
                }
                if let DirLValue::Var(lhs_name) = lhs
                    && self.stack_state_vars.contains(lhs_name)
                    && let DirExpr::Var(rhs_name) = rhs
                    && !self.stack_state_vars.contains(rhs_name)
                {
                    self.barrier_vars.insert(lhs_name.clone());
                }
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => self.visit_expr(expr),
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                self.visit_stmts(body);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.visit_stmt(init);
                }
                if let Some(cond) = cond {
                    self.visit_expr(cond);
                }
                if let Some(update) = update {
                    self.visit_stmt(update);
                }
                self.visit_stmts(body);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
            }
            DirStmt::VaStart { va_list, .. } => self.visit_expr(va_list),
            DirStmt::Return(None)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &DirLValue) {
        match lval {
            DirLValue::Var(_) => {}
            DirLValue::Deref { ptr, .. } => {
                collect_vars_in_expr(ptr, &mut self.barrier_vars);
                self.visit_expr(ptr);
            }
            DirLValue::Index { base, index, .. } => {
                collect_vars_in_expr(base, &mut self.barrier_vars);
                self.visit_expr(base);
                self.visit_expr(index);
            }
            DirLValue::FieldAccess { base, .. } => self.visit_expr(base),
        }
    }

    fn visit_expr(&mut self, expr: &DirExpr) {
        match expr {
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
            DirExpr::Load { ptr, .. } => {
                collect_vars_in_expr(ptr, &mut self.barrier_vars);
                self.visit_expr(ptr);
            }
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::PtrOffset { base: expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. }
            | DirExpr::FieldAccess { base: expr, .. } => self.visit_expr(expr),
            DirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            DirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            DirExpr::Index { base, index, .. } => {
                collect_vars_in_expr(base, &mut self.barrier_vars);
                self.visit_expr(base);
                self.visit_expr(index);
            }
        }
    }

    fn expr_is_load_derived_barrier(&self, expr: &DirExpr) -> bool {
        match expr {
            DirExpr::Load { .. } => true,
            DirExpr::Var(name) => self.barrier_vars.contains(name),
            DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
                self.expr_is_load_derived_barrier(expr)
            }
            DirExpr::Binary { lhs, rhs, .. } => {
                self.expr_is_load_derived_barrier(lhs) || self.expr_is_load_derived_barrier(rhs)
            }
            DirExpr::Call { args, .. } => args
                .iter()
                .any(|arg| self.expr_is_load_derived_barrier(arg)),
            DirExpr::PtrOffset { base, .. }
            | DirExpr::FieldAccess { base, .. }
            | DirExpr::AggregateCopy { src: base, .. } => self.expr_is_load_derived_barrier(base),
            DirExpr::Index { base, index, .. } => {
                self.expr_is_load_derived_barrier(base) || self.expr_is_load_derived_barrier(index)
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_is_load_derived_barrier(cond)
                    || self.expr_is_load_derived_barrier(then_expr)
                    || self.expr_is_load_derived_barrier(else_expr)
            }
            DirExpr::Const(_, _) | DirExpr::AddressOfGlobal(_) => false,
        }
    }
}

fn collect_vars_in_expr(expr: &DirExpr, vars: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => collect_vars_in_expr(expr, vars),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, vars);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_vars_in_expr(cond, vars);
            collect_vars_in_expr(then_expr, vars);
            collect_vars_in_expr(else_expr, vars);
        }
        DirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(base, vars);
            collect_vars_in_expr(index, vars);
        }
        DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn sorted_var_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn transitive_copy_aliases(
    direct_copies: &crate::HashSet<(String, String)>,
    local_names: &HashSet<String>,
    copy_merge_barriers: &HashSet<String>,
    copy_merge_blocked_pairs: &HashSet<(String, String)>,
) -> HashMap<String, String> {
    fn root(parent: &HashMap<String, String>, node: &str) -> String {
        let mut cur = node.to_string();
        loop {
            match parent.get(&cur) {
                Some(p) if p != &cur => cur = p.clone(),
                _ => break,
            }
        }
        cur
    }

    let eligible_copies = direct_copies.iter().filter(|(a, b)| {
        !copy_merge_barriers.contains(a)
            && !copy_merge_barriers.contains(b)
            && local_names.contains(a)
            && local_names.contains(b)
            && !copy_merge_blocked_pairs.contains(&sorted_var_pair(a, b))
            && (is_eligible_for_speculative_merge_by_name(a)
                || is_eligible_for_speculative_merge_by_name(b))
    });

    let mut parent: HashMap<String, String> = HashMap::default();
    for (a, b) in eligible_copies {
        let ra = root(&parent, a);
        let rb = root(&parent, b);
        if ra == rb {
            continue;
        }
        let (keep, drop) = if name_priority(&ra) >= name_priority(&rb) {
            (ra, rb)
        } else {
            (rb, ra)
        };
        parent.insert(drop, keep);
    }

    let mut nodes = HashSet::<String>::default();
    for (a, b) in direct_copies {
        if local_names.contains(a) {
            nodes.insert(a.clone());
        }
        if local_names.contains(b) {
            nodes.insert(b.clone());
        }
    }
    let mut aliases = HashMap::<String, String>::default();
    for node in nodes {
        let canonical = root(&parent, &node);
        if canonical != node {
            aliases.insert(node, canonical);
        }
    }
    aliases
}

fn collect_dominant_copy_join_merges(stmts: &[DirStmt]) -> Vec<(String, String)> {
    let mut renames = Vec::new();
    collect_dominant_copy_join_merges_in_stmts(stmts, &mut renames);
    renames
}

fn collect_dominant_copy_join_merges_in_stmts(
    stmts: &[DirStmt],
    renames: &mut Vec<(String, String)>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if let (Some((dest1, src1)), Some((dest2, src2))) = (
                    last_direct_var_copy(then_body),
                    last_direct_var_copy(else_body),
                ) {
                    if src1 == src2
                        && dest1 != dest2
                        && is_eligible_for_speculative_merge_by_name(&dest1)
                        && is_eligible_for_speculative_merge_by_name(&dest2)
                    {
                        let (keep, drop) = if name_priority(&dest1) >= name_priority(&dest2) {
                            (dest1, dest2)
                        } else {
                            (dest2, dest1)
                        };
                        renames.push((drop, keep));
                    }
                }
                collect_dominant_copy_join_merges_in_stmts(then_body, renames);
                collect_dominant_copy_join_merges_in_stmts(else_body, renames);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_dominant_copy_join_merges_in_stmts(body, renames);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_dominant_copy_join_merges_in_stmts(std::slice::from_ref(init), renames);
                }
                if let Some(update) = update {
                    collect_dominant_copy_join_merges_in_stmts(
                        std::slice::from_ref(update),
                        renames,
                    );
                }
                collect_dominant_copy_join_merges_in_stmts(body, renames);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_dominant_copy_join_merges_in_stmts(&case.body, renames);
                }
                collect_dominant_copy_join_merges_in_stmts(default, renames);
            }
            _ => {}
        }
    }
}

fn last_direct_var_copy(body: &[DirStmt]) -> Option<(String, String)> {
    body.iter().rev().find_map(|stmt| {
        if let DirStmt::Assign {
            lhs: DirLValue::Var(dest),
            rhs: DirExpr::Var(src),
        } = stmt
        {
            Some((dest.clone(), src.clone()))
        } else {
            None
        }
    })
}

fn is_eligible_for_speculative_merge_by_name(name: &str) -> bool {
    if is_hardware_register_variable(name) {
        let name_lower = name.to_lowercase();
        let is_sp_or_bp = matches!(
            name_lower.as_str(),
            "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
        );
        return !is_sp_or_bp
            && (name.starts_with("xVar")
                || name.starts_with("uVar")
                || name.starts_with("iVar")
                || name.starts_with("bVar")
                || name.starts_with("tmp_"));
    }
    name_priority(name) < 2
}

pub fn apply_variable_merge_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;

    // Keep track of parameters to avoid merging them
    let param_names: crate::HashSet<String> =
        func.params.iter().map(|p| p.name.clone()).collect();

    // Step 1: Merge overlapping stack variables (coalescing multiple stack-slot views)
    let mut stack_renames = Vec::new();
    let mut stack_locals = func.locals.clone();
    let mut stack_merged = vec![false; stack_locals.len()];
    let read_vars = collect_read_vars(&func.body);

    for i in 0..stack_locals.len() {
        if stack_merged[i] {
            continue;
        }
        let b1_name = stack_locals[i].name.clone();
        let b1_ty = stack_locals[i].ty.clone();
        let b1_origin = stack_locals[i].origin;
        if param_names.contains(&b1_name) {
            continue;
        }
        let Some((offset1, size1, is_derived1)) = get_stack_span_from_parts(b1_origin, &b1_ty)
        else {
            continue;
        };

        for j in (i + 1)..stack_locals.len() {
            if stack_merged[j] {
                continue;
            }
            let b2_name = stack_locals[j].name.clone();
            let b2_ty = stack_locals[j].ty.clone();
            let b2_origin = stack_locals[j].origin;
            if param_names.contains(&b2_name) {
                continue;
            }
            let Some((offset2, size2, is_derived2)) = get_stack_span_from_parts(b2_origin, &b2_ty)
            else {
                continue;
            };

            let is_slot1 = b1_name.starts_with("slot_");
            let is_slot2 = b2_name.starts_with("slot_");
            let distinct_read_stack_states =
                offset1 != offset2 && read_vars.contains(&b1_name) && read_vars.contains(&b2_name);
            let can_merge = (!is_slot1 && !is_slot2)
                && !distinct_read_stack_states
                && ((is_derived1 == is_derived2
                    && spans_overlap((offset1, size1), (offset2, size2)))
                    || (offset1 == offset2));

            if can_merge {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let (keep_idx, merge_idx, keep_name, merge_name, keep_ty, merge_ty) = if p1 >= p2 {
                    (
                        i,
                        j,
                        b1_name.clone(),
                        b2_name.clone(),
                        b1_ty.clone(),
                        b2_ty.clone(),
                    )
                } else {
                    (
                        j,
                        i,
                        b2_name.clone(),
                        b1_name.clone(),
                        b2_ty.clone(),
                        b1_ty.clone(),
                    )
                };

                stack_renames.push((merge_name, keep_name));

                let unified_ty = force_unify_types_for_merge(&keep_ty, &merge_ty);
                stack_locals[keep_idx].ty = unified_ty;

                stack_merged[merge_idx] = true;
                changed = true;
            }
        }
    }

    if !stack_renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &stack_renames);
        let renamed_from: crate::HashSet<String> =
            stack_renames.iter().map(|(from, _)| from.clone()).collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        for local in &mut func.locals {
            if let Some(updated) = stack_locals.iter().find(|l| l.name == local.name) {
                local.ty = updated.ty.clone();
            }
        }
        changed = true;
    }

    let direct_copies = collect_direct_copies(&func.body);
    let join_copy_merges = collect_dominant_copy_join_merges(&func.body);
    let mut direct_copies = direct_copies;
    for (a, b) in join_copy_merges {
        direct_copies.insert((a.clone(), b.clone()));
        direct_copies.insert((b, a));
    }
    let local_names: HashSet<String> = func.locals.iter().map(|b| b.name.clone()).collect();
    let stack_state_vars: HashSet<String> = func
        .locals
        .iter()
        .filter(|binding| {
            matches!(
                binding.origin,
                Some(
                    NirBindingOrigin::StackOffset(_)
                        | NirBindingOrigin::HomeSlot(_)
                        | NirBindingOrigin::DerivedFromStackOffset(_)
                )
            )
        })
        .map(|binding| binding.name.clone())
        .collect();
    let copy_merge_barriers = collect_copy_merge_barrier_vars(&func.body, stack_state_vars);
    // Pairs that appear together in one expression (e.g. `eax = ecx + edx`) must
    // never be unified — applies to both copy-alias and disjoint live-range merge.
    let copy_merge_blocked_pairs = collect_cooccurring_var_pairs(&func.body);
    let copy_aliases = transitive_copy_aliases(
        &direct_copies,
        &local_names,
        &copy_merge_barriers,
        &copy_merge_blocked_pairs,
    );

    if !copy_aliases.is_empty() {
        let copy_renames: Vec<(String, String)> = copy_aliases.into_iter().collect();
        rename_vars_in_stmts(&mut func.body, &copy_renames);
        let renamed_from: crate::HashSet<String> =
            copy_renames.iter().map(|(from, _)| from.clone()).collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        changed = true;
    }

    // Step 2: Speculatively merge variables with disjoint live ranges and compatible types
    let mut live_ranges = LiveRangeCollector {
        stmt_counter: 0,
        ranges: HashMap::default(),
        labels: HashMap::default(),
        backedges: Vec::new(),
        control_intervals: Vec::new(),
    };
    live_ranges.visit_stmts(&func.body);

    // Apply unstructured loop backedges collected during visit
    let backedges = live_ranges.backedges.clone();
    for (loop_start, loop_end) in backedges {
        live_ranges.extend_loop_ranges(loop_start, loop_end);
    }

    let mut disjoint_renames = Vec::new();
    let mut disjoint_merged = vec![false; func.locals.len()];
    let mut current_locals = func.locals.clone();
    let mut merge_members = current_locals
        .iter()
        .map(|binding| [binding.name.clone()].into_iter().collect::<HashSet<_>>())
        .collect::<Vec<_>>();

    for i in 0..current_locals.len() {
        if disjoint_merged[i] {
            continue;
        }
        let b1_name = current_locals[i].name.clone();
        let b1_ty = current_locals[i].ty.clone();
        let b1_init = current_locals[i].initializer.is_some();
        if param_names.contains(&b1_name) || b1_init {
            continue;
        }
        if copy_merge_barriers.contains(&b1_name) {
            continue;
        }
        if !live_ranges.ranges.contains_key(&b1_name) {
            continue;
        }

        if !is_eligible_for_speculative_merge(&current_locals[i]) {
            continue;
        }
        let span1 = get_stack_span_from_parts(current_locals[i].origin, &b1_ty);

        for j in (i + 1)..current_locals.len() {
            if disjoint_merged[j] {
                continue;
            }
            if !is_eligible_for_speculative_merge(&current_locals[j]) {
                continue;
            }
            let b2_name = current_locals[j].name.clone();
            let b2_ty = current_locals[j].ty.clone();
            let b2_init = current_locals[j].initializer.is_some();
            if param_names.contains(&b2_name) || b2_init {
                continue;
            }
            if copy_merge_barriers.contains(&b2_name) {
                continue;
            }
            let Some(&(start2, end2)) = live_ranges.ranges.get(&b2_name) else {
                continue;
            };
            let Some(&(start1, end1)) = live_ranges.ranges.get(&b1_name) else {
                break;
            };

            // Disjoint Domain Restriction: at least one variable must be a
            // synthetic temporary. Architectural register bindings are never
            // candidates here; their identity may only be shared by an
            // explicit materialize/def-use join proof.
            let is_temp1 =
                current_locals[i].is_temp_like() || name_priority(&current_locals[i].name) <= 1;
            let is_temp2 =
                current_locals[j].is_temp_like() || name_priority(&current_locals[j].name) <= 1;
            if !is_temp1 && !is_temp2 {
                continue;
            }

            // Control-Flow Boundaries: Reject merges across major loop scopes or switch boundaries.
            // If one variable is loop-local and the other is not (inside != inside), reject the merge.
            let crosses_boundary = live_ranges
                .control_intervals
                .iter()
                .any(|&(c_start, c_end)| {
                    let in1 = start1 >= c_start && end1 <= c_end;
                    let in2 = start2 >= c_start && end2 <= c_end;
                    in1 != in2
                });

            if crosses_boundary {
                continue;
            }

            let span2 = get_stack_span_from_parts(current_locals[j].origin, &b2_ty);
            if let (Some((off1, _, _)), Some((off2, _, _))) = (span1, span2) {
                if off1 != off2 {
                    continue;
                }
            }

            // Same barrier as copy-alias merge: co-occurring names are not the
            // same value (saturating_add: a, b, and sum must stay distinct).
            if merge_members[i].iter().any(|left| {
                merge_members[j]
                    .iter()
                    .any(|right| copy_merge_blocked_pairs.contains(&sorted_var_pair(left, right)))
            }) {
                continue;
            }
            // Distinct architectural GPRs are storage identities, not free
            // temps. Coalescing ecx→eax via imprecise live ranges collapses
            // multi-register sequences into `eax + eax`.
            if is_hardware_register_variable(&b1_name)
                && is_hardware_register_variable(&b2_name)
                && !is_sp_or_bp_name(&b1_name)
                && !is_sp_or_bp_name(&b2_name)
            {
                continue;
            }

            let disjoint = end1 < start2 || end2 < start1;
            if !disjoint {
                continue;
            }

            if let Some(unified_ty) = unify_types_for_merge(&b1_ty, &b2_ty) {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let is_stack1 = current_locals[i]
                    .origin
                    .map_or(false, |o| !matches!(o, NirBindingOrigin::Temp));
                let is_stack2 = current_locals[j]
                    .origin
                    .map_or(false, |o| !matches!(o, NirBindingOrigin::Temp));

                let (keep_idx, merge_idx, keep_name, merge_name) = if is_stack1 && !is_stack2 {
                    (i, j, b1_name.clone(), b2_name.clone())
                } else if !is_stack1 && is_stack2 {
                    (j, i, b2_name.clone(), b1_name.clone())
                } else if p1 > p2 {
                    (i, j, b1_name.clone(), b2_name.clone())
                } else if p1 < p2 {
                    (j, i, b2_name.clone(), b1_name.clone())
                } else {
                    if start1 <= start2 {
                        (i, j, b1_name.clone(), b2_name.clone())
                    } else {
                        (j, i, b2_name.clone(), b1_name.clone())
                    }
                };

                disjoint_renames.push((merge_name.clone(), keep_name.clone()));

                current_locals[keep_idx].ty = unified_ty;

                let keep_origin = current_locals[keep_idx].origin;
                let merge_origin = current_locals[merge_idx].origin;
                if (keep_origin.is_none() || matches!(keep_origin, Some(NirBindingOrigin::Temp)))
                    && merge_origin.is_some()
                    && !matches!(merge_origin, Some(NirBindingOrigin::Temp))
                {
                    current_locals[keep_idx].origin = merge_origin;
                }

                let (k_start, k_end) = live_ranges
                    .ranges
                    .get(&keep_name)
                    .copied()
                    .unwrap_or((0, 0));
                let (m_start, m_end) = live_ranges
                    .ranges
                    .get(&merge_name)
                    .copied()
                    .unwrap_or((0, 0));
                live_ranges
                    .ranges
                    .insert(keep_name, (k_start.min(m_start), k_end.max(m_end)));

                let merged_members = merge_members[merge_idx].clone();
                merge_members[keep_idx].extend(merged_members);

                disjoint_merged[merge_idx] = true;
                changed = true;
                if merge_idx == i {
                    break;
                }
            }
        }
    }

    if !disjoint_renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &disjoint_renames);
        let renamed_from: crate::HashSet<String> = disjoint_renames
            .iter()
            .map(|(from, _)| from.clone())
            .collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        for local in &mut func.locals {
            if let Some(updated) = current_locals.iter().find(|l| l.name == local.name) {
                local.ty = updated.ty.clone();
            }
        }
    }

    changed
}

#[derive(Clone, Debug)]
struct VarMeta {
    name: String,
    start: usize,
    end: usize,
}

struct LiveRangeCollector {
    stmt_counter: usize,
    ranges: HashMap<String, (usize, usize)>,
    labels: HashMap<String, usize>,
    backedges: Vec<(usize, usize)>,
    control_intervals: Vec<(usize, usize)>,
}

impl LiveRangeCollector {
    fn visit_stmts(&mut self, stmts: &[DirStmt]) {
        for stmt in stmts {
            self.stmt_counter += 1;
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &DirStmt) {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                self.visit_expr(expr);
            }
            DirStmt::Block(body) => {
                self.visit_stmts(body);
            }
            DirStmt::While { cond, body } => {
                let loop_start = self.stmt_counter;
                self.visit_expr(cond);
                self.visit_stmts(body);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            DirStmt::DoWhile { body, cond } => {
                let loop_start = self.stmt_counter;
                self.visit_stmts(body);
                self.visit_expr(cond);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let loop_start = self.stmt_counter;
                if let Some(init_stmt) = init {
                    self.visit_stmt(init_stmt);
                }
                if let Some(cond_expr) = cond {
                    self.visit_expr(cond_expr);
                }
                if let Some(update_stmt) = update {
                    self.visit_stmt(update_stmt);
                }
                self.visit_stmts(body);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let switch_start = self.stmt_counter;
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
                let switch_end = self.stmt_counter;
                self.control_intervals.push((switch_start, switch_end));
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            DirStmt::VaStart { va_list, .. } => {
                self.visit_expr(va_list);
            }
            DirStmt::Label(name) => {
                self.labels.insert(name.clone(), self.stmt_counter);
            }
            DirStmt::Goto(name) => {
                if let Some(&label_counter) = self.labels.get(name) {
                    if label_counter < self.stmt_counter {
                        self.backedges.push((label_counter, self.stmt_counter));
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &DirLValue) {
        match lval {
            DirLValue::Var(name) => {
                self.mark_seen(name);
            }
            DirLValue::Deref { ptr, .. } => {
                self.visit_expr(ptr);
            }
            DirLValue::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            DirLValue::FieldAccess { base, .. } => {
                self.visit_expr(base);
            }
        }
    }

    fn visit_expr(&mut self, expr: &DirExpr) {
        match expr {
            DirExpr::Var(name) => {
                self.mark_seen(name);
            }
            DirExpr::Cast { expr: inner, .. }
            | DirExpr::Unary { expr: inner, .. }
            | DirExpr::Load { ptr: inner, .. }
            | DirExpr::PtrOffset { base: inner, .. }
            | DirExpr::AggregateCopy { src: inner, .. }
            | DirExpr::FieldAccess { base: inner, .. } => {
                self.visit_expr(inner);
            }
            DirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            DirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            DirExpr::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        }
    }

    fn extend_loop_ranges(&mut self, loop_start: usize, loop_end: usize) {
        for range in self.ranges.values_mut() {
            if range.0 < loop_start && range.1 >= loop_start {
                range.1 = range.1.max(loop_end);
            }
        }
    }

    fn mark_seen(&mut self, name: &str) {
        let idx = self.stmt_counter;
        let entry = self.ranges.entry(name.to_string()).or_insert((idx, idx));
        entry.1 = idx;
    }
}

fn name_priority(name: &str) -> usize {
    if name.starts_with("uVar_dp_") {
        return 0; // lowest priority (dp temp variables)
    }
    if name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
        || name.starts_with("temp_")
        || name.starts_with("temp")
    {
        return 1;
    }
    if name.starts_with("slot_") {
        return 1;
    }
    if name == "result" || name == "retval" {
        return 2;
    }
    3 // highest priority: meaningful recovered symbols
}

fn is_sp_or_bp_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
    )
}

fn is_hardware_register_variable(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let gprs = [
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "rax", "rbx", "rcx", "rdx", "rsi",
        "rdi", "rsp", "rbp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "ax", "bx",
        "cx", "dx", "sp", "bp", "si", "di", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "r8w", "r9w", "r10w", "r11w",
        "r12w", "r13w", "r14w", "r15w", "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b",
        "r15b",
    ];
    if gprs.contains(&name_lower.as_str()) {
        return true;
    }
    if name_lower == "reg" || name_lower.starts_with("reg_") {
        return true;
    }
    if name_lower.starts_with("reg") && name_lower[3..].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if name_lower.starts_with("xmm")
        || name_lower.starts_with("ymm")
        || name_lower.starts_with("zmm")
        || name_lower.starts_with("fvar")
    {
        return true;
    }
    false
}

fn is_eligible_for_speculative_merge(binding: &DirBinding) -> bool {
    if is_hardware_register_variable(&binding.name) {
        // A hardware name carries architectural storage identity even when its
        // binding origin is TempPreserved. Reusing it for a disjoint synthetic
        // range recreates the register-role collapse that materialization's
        // reaching-definition proofs intentionally prevented.
        return false;
    }
    // Symbolic Priority Preservation: Exclude variables with priority >= 2
    // (e.g. result, retval, or meaningful recovered symbols).
    if name_priority(&binding.name) >= 2 {
        return false;
    }
    true
}

fn get_stack_span_from_parts(
    origin: Option<NirBindingOrigin>,
    ty: &NirType,
) -> Option<(i64, u32, bool)> {
    let (offset, is_derived) = match origin {
        Some(NirBindingOrigin::StackOffset(o))
        | Some(NirBindingOrigin::HomeSlot(o))
        | Some(NirBindingOrigin::OutgoingArgSlot(o)) => (o, false),
        Some(NirBindingOrigin::DerivedFromStackOffset(o)) => (o, true),
        _ => return None,
    };
    let size = match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => bits / 8,
        NirType::Ptr(_) => 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Float { bits } => bits / 8,
        NirType::Unknown => 4,
    };
    Some((offset, size, is_derived))
}

fn spans_overlap(s1: (i64, u32), s2: (i64, u32)) -> bool {
    let (off1, sz1) = s1;
    let (off2, sz2) = s2;
    off1 < off2 + sz2 as i64 && off2 < off1 + sz1 as i64
}

fn unify_types_for_merge(t1: &NirType, t2: &NirType) -> Option<NirType> {
    if *t1 == NirType::Unknown {
        return Some(t2.clone());
    }
    if *t2 == NirType::Unknown {
        return Some(t1.clone());
    }
    match (t1, t2) {
        (
            NirType::Int {
                bits: b1,
                signed: s1,
            },
            NirType::Int {
                bits: b2,
                signed: s2,
            },
        ) => Some(NirType::Int {
            bits: (*b1).max(*b2),
            signed: *s1 || *s2,
        }),
        (NirType::Ptr(p1), NirType::Ptr(p2)) => {
            let inner = unify_types_for_merge(p1, p2)?;
            Some(NirType::Ptr(Box::new(inner)))
        }
        (t1, t2) if t1 == t2 => Some(t1.clone()),
        _ => None,
    }
}

fn force_unify_types_for_merge(t1: &NirType, t2: &NirType) -> NirType {
    if let Some(unified) = unify_types_for_merge(t1, t2) {
        return unified;
    }
    let size1 = type_byte_size(t1).unwrap_or(4);
    let size2 = type_byte_size(t2).unwrap_or(4);
    if size1 >= size2 {
        t1.clone()
    } else {
        t2.clone()
    }
}

fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use fission_midend_core::{NirBindingOrigin, NirType};
    use fission_midend_dir::{DirBinding, DirExpr, DirFunction, DirLValue, DirStmt};

    #[test]
    fn test_stack_slot_coalescing_and_domain_separation() {
        // Direct stack offset 1: offset -16, size 4
        let b1 = DirBinding {
            name: "local_10".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-16)),
            initializer: None,
        };

        // Direct stack offset 2: offset -14, size 4 (overlaps with -16)
        let b2 = DirBinding {
            name: "local_0e".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-14)),
            initializer: None,
        };

        // Derived stack offset: offset -8, size 4 (different offset)
        let b3 = DirBinding {
            name: "derived_08".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::DerivedFromStackOffset(-8)),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2, b3],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_10".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_0e".to_string()),
                    rhs: DirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("derived_08".to_string()),
                    rhs: DirExpr::Const(
                        3,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        assert!(changed);

        // local_10 and local_0e should be merged (same domain, overlapping spans)
        // derived_08 should NOT be merged (different domain and different offset)
        let names: Vec<String> = func.locals.iter().map(|l| l.name.clone()).collect();
        assert!(names.contains(&"local_10".to_string()) || names.contains(&"local_0e".to_string()));
        assert!(names.contains(&"derived_08".to_string()));
        assert_eq!(func.locals.len(), 2);
    }

    #[test]
    fn test_speculative_disjoint_merge_type_unification() {
        let b1 = DirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_fn2".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // Disjoint live ranges: temp_1 is used and then dead before temp_2 is used
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_1".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_2".to_string()),
                    rhs: DirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        assert!(changed);

        // They should be merged into temp_1 or temp_2 (since they are disjoint and types are compatible)
        assert_eq!(func.locals.len(), 1);
        // The unified type should be Int
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 32,
                signed: false
            }
        );
    }

    #[test]
    fn copy_merge_preserves_load_address_cursor_temp() {
        let byte_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let cursor_slot = DirBinding {
            name: "local_10".to_string(),
            ty: NirType::Ptr(Box::new(byte_ty.clone())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-16)),
            initializer: None,
        };
        let address_tmp = DirBinding {
            name: "xVar18".to_string(),
            ty: NirType::Ptr(Box::new(byte_ty.clone())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };
        let byte_tmp = DirBinding {
            name: "xVar22".to_string(),
            ty: byte_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "checksum_shape".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![cursor_slot, address_tmp, byte_tmp],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar18".to_string()),
                    rhs: DirExpr::Var("local_10".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar18".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("xVar18".to_string())),
                        rhs: Box::new(DirExpr::Var("param_10".to_string())),
                        ty: NirType::Ptr(Box::new(byte_ty.clone())),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar22".to_string()),
                    rhs: DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("xVar18".to_string())),
                        ty: byte_ty,
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar18".to_string()),
                    rhs: DirExpr::Cast {
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        expr: Box::new(DirExpr::Var("xVar22".to_string())),
                    },
                },
            ],
            ..Default::default()
        };

        apply_variable_merge_pass(&mut func);

        let rendered_vars = format!("{:?}", func.body);
        assert!(
            rendered_vars.contains("xVar18"),
            "load address temp must not be copy-merged into local_10"
        );
        assert!(
            rendered_vars.contains("local_10"),
            "cursor stack slot must remain distinct from the address temp"
        );
    }

    #[test]
    fn copy_merge_preserves_distinct_stack_state_from_shared_register_seed() {
        let int_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let wide_int_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let local_i = DirBinding {
            name: "local_4".to_string(),
            ty: int_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-4)),
            initializer: None,
        };
        let local_j = DirBinding {
            name: "local_8".to_string(),
            ty: wide_int_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-8)),
            initializer: None,
        };
        let rax = DirBinding {
            name: "rax".to_string(),
            ty: int_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: Some(DirExpr::Const(0, int_ty.clone())),
        };

        let mut func = DirFunction {
            name: "rc4_state_shape".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![local_i, local_j, rax],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_4".to_string()),
                    rhs: DirExpr::Var("rax".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_8".to_string()),
                    rhs: DirExpr::Var("rax".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_4".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("local_4".to_string())),
                        rhs: Box::new(DirExpr::Const(1, int_ty.clone())),
                        ty: int_ty.clone(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_8".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("local_8".to_string())),
                        rhs: Box::new(DirExpr::Var("local_4".to_string())),
                        ty: wide_int_ty,
                    },
                },
            ],
            ..Default::default()
        };

        apply_variable_merge_pass(&mut func);

        let rendered_vars = format!("{:?}", func.body);
        assert!(
            rendered_vars.contains("local_4"),
            "loop index stack state must remain distinct"
        );
        assert!(
            rendered_vars.contains("local_8"),
            "RC4 accumulator stack state must remain distinct"
        );
    }

    #[test]
    fn copy_merge_preserves_distinct_temps_that_cooccur_after_seed_copy() {
        let int_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let primary = DirBinding {
            name: "uVar20".to_string(),
            ty: int_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };
        let bias = DirBinding {
            name: "uVar21".to_string(),
            ty: int_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "copy_seed_then_subtract_shape".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![primary, bias],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar21".to_string()),
                    rhs: DirExpr::Var("uVar20".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar20".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("uVar20".to_string())),
                        rhs: Box::new(DirExpr::Var("uVar21".to_string())),
                        ty: int_ty.clone(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar20".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(DirExpr::Var("uVar20".to_string())),
                        rhs: Box::new(DirExpr::Var("uVar21".to_string())),
                        ty: int_ty,
                    },
                },
            ],
            ..Default::default()
        };

        apply_variable_merge_pass(&mut func);

        let rendered_vars = format!("{:?}", func.body);
        assert!(
            rendered_vars.contains("uVar20"),
            "primary temp must remain available after seed copy"
        );
        assert!(
            rendered_vars.contains("uVar21"),
            "seeded temp must not be rewritten into the primary temp when both co-occur"
        );
        assert_eq!(
            func.locals.len(),
            2,
            "copy aliases that co-occur later are distinct live values"
        );
    }

    #[test]
    fn test_loop_live_range_collector() {
        let b1 = DirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_loop_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // temp_1 is defined before the loop
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_1".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                // Loop
                DirStmt::While {
                    cond: DirExpr::Const(1, NirType::Bool),
                    body: vec![
                        // Inside the loop, temp_1 is read
                        DirStmt::Assign {
                            lhs: DirLValue::Var("dummy".to_string()),
                            rhs: DirExpr::Var("temp_1".to_string()),
                        },
                        // temp_2 is defined and read inside the loop, after temp_1's read
                        DirStmt::Assign {
                            lhs: DirLValue::Var("temp_2".to_string()),
                            rhs: DirExpr::Const(
                                2,
                                NirType::Int {
                                    bits: 32,
                                    signed: false,
                                },
                            ),
                        },
                        DirStmt::Assign {
                            lhs: DirLValue::Var("dummy2".to_string()),
                            rhs: DirExpr::Var("temp_2".to_string()),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        // They should NOT be merged because temp_1 is live across the entire loop body,
        // which overlaps with temp_2.
        assert!(!changed);
        assert_eq!(func.locals.len(), 2);
    }

    #[test]
    fn test_unstructured_loop_live_range_collector() {
        let b1 = DirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_loop_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // temp_1 is defined before the loop
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_1".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                // Loop header label
                DirStmt::Label("loop_start".to_string()),
                // Inside the loop, temp_1 is read
                DirStmt::Assign {
                    lhs: DirLValue::Var("dummy".to_string()),
                    rhs: DirExpr::Var("temp_1".to_string()),
                },
                // temp_2 is defined and read inside the loop, after temp_1's read
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_2".to_string()),
                    rhs: DirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("dummy2".to_string()),
                    rhs: DirExpr::Var("temp_2".to_string()),
                },
                // Loop backedge goto
                DirStmt::Goto("loop_start".to_string()),
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        // They should NOT be merged because temp_1 is live across the entire unstructured loop body,
        // which overlaps with temp_2.
        assert!(!changed);
        assert_eq!(func.locals.len(), 2);
    }

    #[test]
    fn test_speculative_merging_symbolic_priority_guard() {
        let b1 = DirBinding {
            name: "result".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "retval".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_pri_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("result".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("retval".to_string()),
                    rhs: DirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        // Should NOT merge because both result and retval have priority >= 2.
        assert!(!changed);
        assert_eq!(func.locals.len(), 2);
    }

    #[test]
    fn test_speculative_merging_disjoint_domain_guard() {
        let b1 = DirBinding {
            name: "local_10".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-16)),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "local_20".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-32)),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_domain_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_10".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_20".to_string()),
                    rhs: DirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        // Should NOT merge because neither variable is a hardware temporary (both are stack slots).
        assert!(!changed);
        assert_eq!(func.locals.len(), 2);
    }

    #[test]
    fn test_speculative_merging_control_flow_boundary_guard() {
        let b1 = DirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = DirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = DirFunction {
            name: "test_ctrl_fn".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                // temp_1 defined before the loop
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_1".to_string()),
                    rhs: DirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                },
                DirStmt::While {
                    cond: DirExpr::Const(1, NirType::Bool),
                    body: vec![
                        // temp_2 defined inside the loop
                        DirStmt::Assign {
                            lhs: DirLValue::Var("temp_2".to_string()),
                            rhs: DirExpr::Const(
                                2,
                                NirType::Int {
                                    bits: 32,
                                    signed: false,
                                },
                            ),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        let changed1 = apply_variable_merge_pass(&mut func);
        // Should NOT merge because temp_2 starts inside the loop, and no direct copy links them.
        assert!(!changed1);
        assert_eq!(func.locals.len(), 2);
    }

    /// saturating_add / multi-register arithmetic: co-occurring GPRs must not
    /// collapse into one name (`eax + eax`).
    #[test]
    fn variable_merge_preserves_cooccurring_hw_gprs_in_add() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let bind = |name: &str| DirBinding {
            name: name.to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        };
        let mut func = DirFunction {
            name: "sat_like".to_string(),
            params: vec![
                DirBinding {
                    name: "param_1".to_string(),
                    ty: i32_ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(0)),
                    initializer: None,
                },
                DirBinding {
                    name: "param_2".to_string(),
                    ty: i32_ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(1)),
                    initializer: None,
                },
            ],
            locals: vec![bind("ecx"), bind("edx"), bind("eax")],
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("ecx".to_string()),
                    rhs: DirExpr::Var("param_1".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("edx".to_string()),
                    rhs: DirExpr::Var("param_2".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("eax".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("ecx".to_string())),
                        rhs: Box::new(DirExpr::Var("edx".to_string())),
                        ty: i32_ty.clone(),
                    },
                },
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::SLt,
                        lhs: Box::new(DirExpr::Var("eax".to_string())),
                        rhs: Box::new(DirExpr::Var("ecx".to_string())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![DirStmt::Return(Some(DirExpr::Const(
                        2147483647,
                        i32_ty.clone(),
                    )))],
                    else_body: vec![],
                },
                DirStmt::Return(Some(DirExpr::Var("eax".to_string()))),
            ],
            return_type: i32_ty,
            ..Default::default()
        };

        let _ = apply_variable_merge_pass(&mut func);
        let code = format!("{func:?}");
        assert!(
            !code.contains("eax + eax") && !code.contains("eax+eax"),
            "co-occurring GPRs collapsed:\n{code}"
        );
        // Prefer keeping distinct param sources or distinct regs.
        let has_distinct_add = code.contains("ecx") && code.contains("edx")
            || code.contains("param_1") && code.contains("param_2");
        assert!(
            has_distinct_add || code.contains("+"),
            "expected multi-register sum form:\n{code}"
        );
        assert!(
            !(code.contains("eax = param_1")
                && code.contains("eax = param_2")
                && code.matches("eax = param").count() >= 2),
            "both params forced onto eax:\n{code}"
        );
    }

    #[test]
    fn speculative_merge_cannot_reuse_hw_register_for_disjoint_temp_range() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = DirFunction {
            name: "definition_scoped_registers".to_string(),
            locals: vec![
                DirBinding {
                    name: "edx".to_string(),
                    ty: i32_ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::TempPreserved),
                    initializer: None,
                },
                DirBinding {
                    name: "uVar1".to_string(),
                    ty: i32_ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("edx".to_string()),
                    rhs: DirExpr::Const(1, i32_ty.clone()),
                },
                DirStmt::Expr(DirExpr::Var("edx".to_string())),
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar1".to_string()),
                    rhs: DirExpr::Const(2, i32_ty.clone()),
                },
                DirStmt::Return(Some(DirExpr::Var("uVar1".to_string()))),
            ],
            return_type: i32_ty,
            ..Default::default()
        };

        assert!(!apply_variable_merge_pass(&mut func));
        assert!(func.locals.iter().any(|binding| binding.name == "edx"));
        assert!(func.locals.iter().any(|binding| binding.name == "uVar1"));
    }

    #[test]
    fn speculative_merge_respects_blocked_pairs_across_transitive_groups() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let binding = |name: &str| DirBinding {
            name: name.to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = DirFunction {
            name: "transitive_live_ranges".to_string(),
            locals: vec![
                binding("temp_early"),
                binding("temp_left"),
                binding("temp_right"),
            ],
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_early".to_string()),
                    rhs: DirExpr::Const(0, i32_ty.clone()),
                },
                DirStmt::Expr(DirExpr::Var("temp_early".to_string())),
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_left".to_string()),
                    rhs: DirExpr::Const(7, i32_ty.clone()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("temp_right".to_string()),
                    rhs: DirExpr::Const(3, i32_ty.clone()),
                },
                DirStmt::Return(Some(DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    lhs: Box::new(DirExpr::Var("temp_left".to_string())),
                    rhs: Box::new(DirExpr::Var("temp_right".to_string())),
                    ty: i32_ty.clone(),
                })),
            ],
            return_type: i32_ty,
            ..Default::default()
        };

        assert!(apply_variable_merge_pass(&mut func));
        let DirStmt::Return(Some(DirExpr::Binary { lhs, rhs, .. })) = &func.body[4] else {
            panic!("expected binary return");
        };
        assert_ne!(lhs, rhs, "co-occurring group members were merged");
    }
}
