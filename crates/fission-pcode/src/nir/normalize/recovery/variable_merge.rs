use super::super::*;// For accessing normalizer structures
use crate::nir::var_rename::rename_vars_in_stmts;
use std::collections::HashMap;

fn collect_direct_copies(stmts: &[HirStmt]) -> std::collections::HashSet<(String, String)> {
    let mut copies = std::collections::HashSet::new();
    fn visit(stmts: &[HirStmt], copies: &mut std::collections::HashSet<(String, String)>) {
        for stmt in stmts {
            match stmt {
                HirStmt::Assign {
                    lhs: HirLValue::Var(lhs_name),
                    rhs: HirExpr::Var(rhs_name),
                } => {
                    copies.insert((lhs_name.clone(), rhs_name.clone()));
                    copies.insert((rhs_name.clone(), lhs_name.clone()));
                }
                HirStmt::Block(body)
                | HirStmt::While { body, .. }
                | HirStmt::DoWhile { body, .. } => {
                    visit(body, copies);
                }
                HirStmt::If { then_body, else_body, .. } => {
                    visit(then_body, copies);
                    visit(else_body, copies);
                }
                HirStmt::For { init, update, body, .. } => {
                    if let Some(init_stmt) = init {
                        visit(std::slice::from_ref(init_stmt), copies);
                    }
                    if let Some(update_stmt) = update {
                        visit(std::slice::from_ref(update_stmt), copies);
                    }
                    visit(body, copies);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(&case.body, copies);
                    }
                    visit(default, copies);
                }
                _ => {}
            }
        }
    }
    visit(stmts, &mut copies);
    copies
}

pub(crate) fn apply_variable_merge_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // Keep track of parameters to avoid merging them
    let param_names: std::collections::HashSet<String> = func
        .params
        .iter()
        .map(|p| p.name.clone())
        .collect();

    // Step 1: Merge overlapping stack variables (coalescing multiple stack-slot views)
    let mut stack_renames = Vec::new();
    let mut stack_locals = func.locals.clone();
    let mut stack_merged = vec![false; stack_locals.len()];

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
        let Some((offset1, size1, is_derived1)) = get_stack_span_from_parts(b1_origin, &b1_ty) else {
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
            let Some((offset2, size2, is_derived2)) = get_stack_span_from_parts(b2_origin, &b2_ty) else {
                continue;
            };

            let is_slot1 = b1_name.starts_with("slot_");
            let is_slot2 = b2_name.starts_with("slot_");
            let can_merge = (!is_slot1 && !is_slot2) && (
                (is_derived1 == is_derived2 && spans_overlap((offset1, size1), (offset2, size2)))
                || (offset1 == offset2)
            );

            if can_merge {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let (keep_idx, merge_idx, keep_name, merge_name, keep_ty, merge_ty) = if p1 >= p2 {
                    (i, j, b1_name.clone(), b2_name.clone(), b1_ty.clone(), b2_ty.clone())
                } else {
                    (j, i, b2_name.clone(), b1_name.clone(), b2_ty.clone(), b1_ty.clone())
                };

                eprintln!("DEBUG STACK MERGE: merging {} into {}", merge_name, keep_name);
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
        let renamed_from: std::collections::HashSet<String> = stack_renames
            .iter()
            .map(|(from, _)| from.clone())
            .collect();
        func.locals.retain(|local| !renamed_from.contains(&local.name));
        for local in &mut func.locals {
            if let Some(updated) = stack_locals.iter().find(|l| l.name == local.name) {
                local.ty = updated.ty.clone();
            }
        }
    }

    let direct_copies = collect_direct_copies(&func.body);

    // Step 2: Speculatively merge variables with disjoint live ranges and compatible types
    let mut live_ranges = LiveRangeCollector {
        stmt_counter: 0,
        ranges: HashMap::new(),
        labels: HashMap::new(),
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
        let Some(&(start1, end1)) = live_ranges.ranges.get(&b1_name) else {
            continue;
        };

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
            let Some(&(start2, end2)) = live_ranges.ranges.get(&b2_name) else {
                continue;
            };

            // Disjoint Domain Restriction: At least one variable must be a hardware temporary
            let is_temp1 = current_locals[i].is_temp_like() || name_priority(&current_locals[i].name) <= 1;
            let is_temp2 = current_locals[j].is_temp_like() || name_priority(&current_locals[j].name) <= 1;
            if !is_temp1 && !is_temp2 {
                continue;
            }

            // Control-Flow Boundaries: Reject merges across major loop scopes or switch boundaries.
            // If one variable is loop-local and the other is not (inside != inside), reject the merge.
            let crosses_boundary = live_ranges.control_intervals.iter().any(|&(c_start, c_end)| {
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

            let disjoint = end1 < start2 || end2 < start1;
            if !disjoint {
                continue;
            }

            if let Some(unified_ty) = unify_types_for_merge(&b1_ty, &b2_ty) {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let is_stack1 = current_locals[i].origin.map_or(false, |o| {
                    !matches!(o, NirBindingOrigin::Temp)
                });
                let is_stack2 = current_locals[j].origin.map_or(false, |o| {
                    !matches!(o, NirBindingOrigin::Temp)
                });

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

                let (k_start, k_end) = live_ranges.ranges.get(&keep_name).copied().unwrap_or((0, 0));
                let (m_start, m_end) = live_ranges.ranges.get(&merge_name).copied().unwrap_or((0, 0));
                live_ranges.ranges.insert(
                    keep_name,
                    (k_start.min(m_start), k_end.max(m_end)),
                );

                disjoint_merged[merge_idx] = true;
                changed = true;
            }
        }
    }

    if !disjoint_renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &disjoint_renames);
        let renamed_from: std::collections::HashSet<String> = disjoint_renames
            .iter()
            .map(|(from, _)| from.clone())
            .collect();
        func.locals.retain(|local| !renamed_from.contains(&local.name));
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
    fn visit_stmts(&mut self, stmts: &[HirStmt]) {
        for stmt in stmts {
            self.stmt_counter += 1;
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                self.visit_expr(expr);
            }
            HirStmt::Block(body) => {
                self.visit_stmts(body);
            }
            HirStmt::While { cond, body } => {
                let loop_start = self.stmt_counter;
                self.visit_expr(cond);
                self.visit_stmts(body);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            HirStmt::DoWhile { body, cond } => {
                let loop_start = self.stmt_counter;
                self.visit_stmts(body);
                self.visit_expr(cond);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            HirStmt::For { init, cond, update, body } => {
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
            HirStmt::Switch { expr, cases, default } => {
                let switch_start = self.stmt_counter;
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
                let switch_end = self.stmt_counter;
                self.control_intervals.push((switch_start, switch_end));
            }
            HirStmt::If { cond, then_body, else_body } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            HirStmt::VaStart { va_list, .. } => {
                self.visit_expr(va_list);
            }
            HirStmt::Label(name) => {
                self.labels.insert(name.clone(), self.stmt_counter);
            }
            HirStmt::Goto(name) => {
                if let Some(&label_counter) = self.labels.get(name) {
                    if label_counter < self.stmt_counter {
                        self.backedges.push((label_counter, self.stmt_counter));
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &HirLValue) {
        match lval {
            HirLValue::Var(name) => {
                self.mark_seen(name);
            }
            HirLValue::Deref { ptr, .. } => {
                self.visit_expr(ptr);
            }
            HirLValue::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            HirLValue::FieldAccess { base, .. } => {
                self.visit_expr(base);
            }
        }
    }

    fn visit_expr(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::Var(name) => {
                self.mark_seen(name);
            }
            HirExpr::Cast { expr: inner, .. }
            | HirExpr::Unary { expr: inner, .. }
            | HirExpr::Load { ptr: inner, .. }
            | HirExpr::PtrOffset { base: inner, .. }
            | HirExpr::AggregateCopy { src: inner, .. }
            | HirExpr::FieldAccess { base: inner, .. } => {
                self.visit_expr(inner);
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            HirExpr::Select { cond, then_expr, else_expr, .. } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            HirExpr::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
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
    if name.starts_with("uVar") || name.starts_with("iVar") || name.starts_with("xVar") || name.starts_with("bVar") || name.starts_with("temp_") || name.starts_with("temp") {
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

fn is_hardware_register_variable(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let gprs = [
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp",
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rsp", "rbp",
        "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
        "ax", "bx", "cx", "dx", "sp", "bp", "si", "di",
        "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh",
        "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
        "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w",
        "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b", "r15b",
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
    if name_lower.starts_with("xmm") || name_lower.starts_with("ymm") || name_lower.starts_with("zmm") || name_lower.starts_with("fvar") {
        return true;
    }
    false
}

fn is_eligible_for_speculative_merge(binding: &NirBinding) -> bool {
    if is_hardware_register_variable(&binding.name) {
        // Permit speculative merging for hardware registers if they represent
        // temporary variables (Temp or TempPreserved) and are not stack/frame pointers.
        let name_lower = binding.name.to_lowercase();
        let is_sp_or_bp = matches!(
            name_lower.as_str(),
            "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
        );
        if !is_sp_or_bp && binding.is_temp_like() {
            return true;
        }
        return false;
    }
    // Symbolic Priority Preservation: Exclude variables with priority >= 2
    // (e.g. result, retval, or meaningful recovered symbols).
    if name_priority(&binding.name) >= 2 {
        return false;
    }
    true
}

fn get_stack_span_from_parts(origin: Option<NirBindingOrigin>, ty: &NirType) -> Option<(i64, u32, bool)> {
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
        (NirType::Int { bits: b1, signed: s1 }, NirType::Int { bits: b2, signed: s2 }) => {
            Some(NirType::Int {
                bits: (*b1).max(*b2),
                signed: *s1 || *s2,
            })
        }
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
    use crate::nir::{NirBinding, NirBindingOrigin, NirType, HirFunction, HirStmt, HirLValue, HirExpr};

    #[test]
    fn test_stack_slot_coalescing_and_domain_separation() {
        // Direct stack offset 1: offset -16, size 4
        let b1 = NirBinding {
            name: "local_10".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-16)),
            initializer: None,
        };

        // Direct stack offset 2: offset -14, size 4 (overlaps with -16)
        let b2 = NirBinding {
            name: "local_0e".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-14)),
            initializer: None,
        };

        // Derived stack offset: offset -8, size 4 (different offset)
        let b3 = NirBinding {
            name: "derived_08".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::DerivedFromStackOffset(-8)),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2, b3],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_10".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_0e".to_string()),
                    rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("derived_08".to_string()),
                    rhs: HirExpr::Const(3, NirType::Int { bits: 32, signed: false }),
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
        let b1 = NirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_fn2".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // Disjoint live ranges: temp_1 is used and then dead before temp_2 is used
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_1".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_2".to_string()),
                    rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
                },
            ],
            ..Default::default()
        };

        let changed = apply_variable_merge_pass(&mut func);
        assert!(changed);

        // They should be merged into temp_1 or temp_2 (since they are disjoint and types are compatible)
        assert_eq!(func.locals.len(), 1);
        // The unified type should be Int
        assert_eq!(func.locals[0].ty, NirType::Int { bits: 32, signed: false });
    }

    #[test]
    fn test_loop_live_range_collector() {
        let b1 = NirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_loop_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // temp_1 is defined before the loop
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_1".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                // Loop
                HirStmt::While {
                    cond: HirExpr::Const(1, NirType::Bool),
                    body: vec![
                        // Inside the loop, temp_1 is read
                        HirStmt::Assign {
                            lhs: HirLValue::Var("dummy".to_string()),
                            rhs: HirExpr::Var("temp_1".to_string()),
                        },
                        // temp_2 is defined and read inside the loop, after temp_1's read
                        HirStmt::Assign {
                            lhs: HirLValue::Var("temp_2".to_string()),
                            rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("dummy2".to_string()),
                            rhs: HirExpr::Var("temp_2".to_string()),
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
        let b1 = NirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_loop_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                // temp_1 is defined before the loop
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_1".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                // Loop header label
                HirStmt::Label("loop_start".to_string()),
                // Inside the loop, temp_1 is read
                HirStmt::Assign {
                    lhs: HirLValue::Var("dummy".to_string()),
                    rhs: HirExpr::Var("temp_1".to_string()),
                },
                // temp_2 is defined and read inside the loop, after temp_1's read
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_2".to_string()),
                    rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("dummy2".to_string()),
                    rhs: HirExpr::Var("temp_2".to_string()),
                },
                // Loop backedge goto
                HirStmt::Goto("loop_start".to_string()),
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
        let b1 = NirBinding {
            name: "result".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "retval".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_pri_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("result".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("retval".to_string()),
                    rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
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
        let b1 = NirBinding {
            name: "local_10".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-16)),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "local_20".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-32)),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_domain_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_10".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_20".to_string()),
                    rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
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
        let b1 = NirBinding {
            name: "temp_1".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let b2 = NirBinding {
            name: "temp_2".to_string(),
            ty: NirType::Int { bits: 32, signed: false },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };

        let mut func = HirFunction {
            name: "test_ctrl_fn".to_string(),
            params: vec![],
            locals: vec![b1, b2],
            body: vec![
                // temp_1 defined before the loop
                HirStmt::Assign {
                    lhs: HirLValue::Var("temp_1".to_string()),
                    rhs: HirExpr::Const(1, NirType::Int { bits: 32, signed: false }),
                },
                HirStmt::While {
                    cond: HirExpr::Const(1, NirType::Bool),
                    body: vec![
                        // temp_2 defined inside the loop
                        HirStmt::Assign {
                            lhs: HirLValue::Var("temp_2".to_string()),
                            rhs: HirExpr::Const(2, NirType::Int { bits: 32, signed: false }),
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
}
