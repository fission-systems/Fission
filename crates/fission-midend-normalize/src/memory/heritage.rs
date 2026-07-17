use super::super::global_opt::{AliasKey, MemDef, MemPhi, MemUse, build_mem_ssa, nir_byte_size};
use crate::prelude::*;
use super::partition_key_for_pointer_expr;
use std::collections::{HashMap, HashSet};

/// Incremental Memory Heritage Solver pass.
///
/// Promotes eligible stack memory locations (represented by non-escaping PartitionKeys)
/// to versioned virtual SSA register variables in `func.locals`, replacing all
/// memory load/store operations on them with direct variable accesses and inserting
/// phi-assignments at block merges.
pub fn apply_memory_heritage(func: &mut HirFunction) -> bool {
    let mem_ssa = build_mem_ssa(func);

    // Identify candidate AliasKeys that are promotable stack slots (non-escaping)
    let mut promotable_keys = HashSet::new();
    for def in &mem_ssa.defs {
        if !def.may_escape {
            if let AliasKey::Partition(partition) = &def.key {
                if partition.is_promotable_stack_like() {
                    promotable_keys.insert(def.key.clone());
                }
            }
        }
    }

    if promotable_keys.is_empty() {
        return false;
    }

    // Allocate versioned variable names for each def/phi ID of promotable keys
    let mut var_names = HashMap::new(); // maps (AliasKey, id) -> variable name String
    let mut var_types = HashMap::new();

    for def in &mem_ssa.defs {
        if promotable_keys.contains(&def.key) {
            let AliasKey::Partition(partition) = &def.key else {
                continue;
            };
            let size = (partition.offset_interval.1 - partition.offset_interval.0).max(1) as u32;
            let ty = NirType::Int {
                bits: size * 8,
                signed: false,
            };
            let base_name = format!("{}_{}", partition.base_object, partition.offset_interval.0);
            // Replace invalid characters for C identifiers
            let base_name = base_name.replace(['.', ' ', '[', ']', '-', '+', '*', '/'], "_");
            let var_name = format!("vVar_{}_v{}", base_name, def.id);

            var_names.insert((def.key.clone(), def.id), var_name.clone());
            var_types.insert(var_name.clone(), ty.clone());
        }
    }

    for phi in &mem_ssa.phis {
        if promotable_keys.contains(&phi.key) {
            let AliasKey::Partition(partition) = &phi.key else {
                continue;
            };
            let size = (partition.offset_interval.1 - partition.offset_interval.0).max(1) as u32;
            let ty = NirType::Int {
                bits: size * 8,
                signed: false,
            };
            let base_name = format!("{}_{}", partition.base_object, partition.offset_interval.0);
            let base_name = base_name.replace(['.', ' ', '[', ']', '-', '+', '*', '/'], "_");
            let var_name = format!("vVar_{}_phi{}", base_name, phi.id);

            var_names.insert((phi.key.clone(), phi.id), var_name.clone());
            var_types.insert(var_name.clone(), ty.clone());
        }
    }

    // Register all new versioned variables in func.locals
    let mut new_locals = Vec::new();
    for (name, ty) in &var_types {
        new_locals.push(NirBinding {
            name: name.clone(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
    }
    // Sort for determinism
    new_locals.sort_by(|a, b| a.name.cmp(&b.name));
    func.locals.extend(new_locals);

    // Rewrite statements and expressions, inserting phi assignments at merges
    let mut rewriter = Rewriter {
        promotable_keys: &promotable_keys,
        var_names: &var_names,
        defs: &mem_ssa.defs,
        uses: &mem_ssa.uses,
        phis: &mem_ssa.phis,
        current_def_idx: 0,
        current_use_idx: 0,
        current_phi_idx: 0,
    };

    let mut body = std::mem::take(&mut func.body);
    rewriter.rewrite_stmts(&mut body);
    func.body = body;

    true
}

struct Rewriter<'a> {
    promotable_keys: &'a HashSet<AliasKey>,
    var_names: &'a HashMap<(AliasKey, usize), String>,
    defs: &'a [MemDef],
    uses: &'a [MemUse],
    phis: &'a [MemPhi],
    current_def_idx: usize,
    current_use_idx: usize,
    current_phi_idx: usize,
}

impl<'a> Rewriter<'a> {
    fn rewrite_stmts(&mut self, stmts: &mut Vec<HirStmt>) {
        let mut i = 0;
        while i < stmts.len() {
            let to_insert = self.rewrite_stmt(&mut stmts[i]);
            if !to_insert.is_empty() {
                let insert_len = to_insert.len();
                for (offset, stmt) in to_insert.into_iter().enumerate() {
                    stmts.insert(i + offset, stmt);
                }
                i += insert_len;
            }
            i += 1;
        }
    }

    fn rewrite_stmt(&mut self, stmt: &mut HirStmt) -> Vec<HirStmt> {
        let mut pre_insert = Vec::new();
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                // Rewrite rhs uses first
                self.rewrite_expr(rhs);

                // Rewrite lhs store if it matches a promotable slot
                let mut is_promoted = false;
                let mut new_var_name = None;

                match lhs {
                    HirLValue::Deref { ptr, ty } => {
                        let size = nir_byte_size(ty);
                        let key = alias_key_for_ptr(ptr, size);
                        if self.promotable_keys.contains(&key) {
                            // Find the matching MemDef
                            while self.current_def_idx < self.defs.len()
                                && self.defs[self.current_def_idx].key != key
                            {
                                self.current_def_idx += 1;
                            }
                            if self.current_def_idx < self.defs.len() {
                                let def = &self.defs[self.current_def_idx];
                                if let Some(var_name) = self.var_names.get(&(key, def.id)) {
                                    new_var_name = Some(var_name.clone());
                                    is_promoted = true;
                                }
                                self.current_def_idx += 1;
                            }
                        }
                    }
                    HirLValue::Index {
                        base,
                        index: _,
                        elem_ty,
                    } => {
                        let size = nir_byte_size(elem_ty);
                        let key = alias_key_for_ptr(base, size);
                        if self.promotable_keys.contains(&key) {
                            while self.current_def_idx < self.defs.len()
                                && self.defs[self.current_def_idx].key != key
                            {
                                self.current_def_idx += 1;
                            }
                            if self.current_def_idx < self.defs.len() {
                                let def = &self.defs[self.current_def_idx];
                                if let Some(var_name) = self.var_names.get(&(key, def.id)) {
                                    new_var_name = Some(var_name.clone());
                                    is_promoted = true;
                                }
                                self.current_def_idx += 1;
                            }
                        }
                    }
                    _ => {}
                }

                if is_promoted {
                    if let Some(var_name) = new_var_name {
                        *lhs = HirLValue::Var(var_name);
                    }
                }
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                self.rewrite_expr(expr);
            }
            HirStmt::Block(body) => {
                self.rewrite_stmts(body);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.rewrite_expr(cond);

                // Rewrite then/else branches
                self.rewrite_stmts(then_body);
                self.rewrite_stmts(else_body);

                // Find MemPhis created by this If branch merge.
                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                // Insert phi assignments at the end of each branch.
                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let then_input = phi.inputs[0];
                        let else_input = phi.inputs[1];

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            // then branch assignment: phi_var = then_input_var
                            if let Some(then_var) =
                                self.var_names.get(&(phi.key.clone(), then_input))
                            {
                                append_to_body_before_cf(
                                    then_body,
                                    HirStmt::Assign {
                                        lhs: HirLValue::Var(phi_var.clone()),
                                        rhs: HirExpr::Var(then_var.clone()),
                                    },
                                );
                            }
                            // else branch assignment: phi_var = else_input_var
                            if let Some(else_var) =
                                self.var_names.get(&(phi.key.clone(), else_input))
                            {
                                append_to_body_before_cf(
                                    else_body,
                                    HirStmt::Assign {
                                        lhs: HirLValue::Var(phi_var.clone()),
                                        rhs: HirExpr::Var(else_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            HirStmt::While { cond, body } => {
                self.rewrite_expr(cond);

                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                self.rewrite_stmts(body);

                // Insert phi-initializations before the loop, and loop-carried updates at the end of body.
                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0]; // loop body end
                        let pre_input = phi.inputs[1]; // before loop

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            // Pre-loop: phi_var = pre_input_var
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(HirStmt::Assign {
                                    lhs: HirLValue::Var(phi_var.clone()),
                                    rhs: HirExpr::Var(pre_var.clone()),
                                });
                            }
                            // Loop end: phi_var = body_input_var
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    body,
                                    HirStmt::Assign {
                                        lhs: HirLValue::Var(phi_var.clone()),
                                        rhs: HirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            HirStmt::DoWhile { body, cond } => {
                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                self.rewrite_stmts(body);
                self.rewrite_expr(cond);

                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0];
                        let pre_input = phi.inputs[1];

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(HirStmt::Assign {
                                    lhs: HirLValue::Var(phi_var.clone()),
                                    rhs: HirExpr::Var(pre_var.clone()),
                                });
                            }
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    body,
                                    HirStmt::Assign {
                                        lhs: HirLValue::Var(phi_var.clone()),
                                        rhs: HirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(s) = init {
                    let mut dummy = vec![(**s).clone()];
                    self.rewrite_stmts(&mut dummy);
                    if dummy.len() == 1 {
                        *s = Box::new(dummy.remove(0));
                    } else if !dummy.is_empty() {
                        *s = Box::new(HirStmt::Block(dummy));
                    }
                }
                if let Some(e) = cond {
                    self.rewrite_expr(e);
                }

                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                self.rewrite_stmts(body);

                if let Some(s) = update {
                    let mut dummy = vec![(**s).clone()];
                    self.rewrite_stmts(&mut dummy);
                    if dummy.len() == 1 {
                        *s = Box::new(dummy.remove(0));
                    } else if !dummy.is_empty() {
                        *s = Box::new(HirStmt::Block(dummy));
                    }
                }

                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0];
                        let pre_input = phi.inputs[1];

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(HirStmt::Assign {
                                    lhs: HirLValue::Var(phi_var.clone()),
                                    rhs: HirExpr::Var(pre_var.clone()),
                                });
                            }
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    body,
                                    HirStmt::Assign {
                                        lhs: HirLValue::Var(phi_var.clone()),
                                        rhs: HirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.rewrite_expr(expr);

                for case in cases.iter_mut() {
                    self.rewrite_stmts(&mut case.body);
                }
                self.rewrite_stmts(default);

                // Collect MemPhis for the switch
                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                // Group merge_phis by key and process
                let mut phis_by_key: HashMap<AliasKey, Vec<MemPhi>> = HashMap::new();
                for phi in merge_phis {
                    phis_by_key.entry(phi.key.clone()).or_default().push(phi);
                }

                for (key, phis) in phis_by_key {
                    let AliasKey::Partition(ref partition) = key else {
                        continue;
                    };
                    let base_name =
                        format!("{}_{}", partition.base_object, partition.offset_interval.0);
                    let base_name =
                        base_name.replace(['.', ' ', '[', ']', '-', '+', '*', '/'], "_");

                    if let Some(first_phi) = phis.first() {
                        let d_saved = first_phi.inputs[0];
                        let pre_var = self.var_names.get(&(key.clone(), d_saved));

                        // The last phi's ID is the final merged variable
                        if let Some(last_phi) = phis.last() {
                            if let Some(phi_var) = self.var_names.get(&(key.clone(), last_phi.id)) {
                                // Assign for each case arm
                                for case in cases.iter_mut() {
                                    let arm_var = find_last_def_in_stmts(&case.body, &base_name)
                                        .or_else(|| pre_var.cloned());
                                    if let Some(v) = arm_var {
                                        append_to_body_before_cf(
                                            &mut case.body,
                                            HirStmt::Assign {
                                                lhs: HirLValue::Var(phi_var.clone()),
                                                rhs: HirExpr::Var(v),
                                            },
                                        );
                                    }
                                }
                                // Assign for default arm
                                let default_var = find_last_def_in_stmts(default, &base_name)
                                    .or_else(|| pre_var.cloned());
                                if let Some(v) = default_var {
                                    append_to_body_before_cf(
                                        default,
                                        HirStmt::Assign {
                                            lhs: HirLValue::Var(phi_var.clone()),
                                            rhs: HirExpr::Var(v),
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        pre_insert
    }

    fn rewrite_expr(&mut self, expr: &mut HirExpr) {
        match expr {
            HirExpr::Load { ptr, ty } => {
                let size = nir_byte_size(ty);
                let key = alias_key_for_ptr(ptr, size);

                let mut is_promoted = false;
                let mut promoted_var = None;

                if self.promotable_keys.contains(&key) {
                    // Find the matching MemUse
                    while self.current_use_idx < self.uses.len()
                        && self.uses[self.current_use_idx].key != key
                    {
                        self.current_use_idx += 1;
                    }
                    if self.current_use_idx < self.uses.len() {
                        let use_node = &self.uses[self.current_use_idx];
                        if let Some(reaching_id) = use_node.reaching_def {
                            if let Some(var_name) = self.var_names.get(&(key.clone(), reaching_id))
                            {
                                promoted_var = Some(var_name.clone());
                                is_promoted = true;
                            }
                        }
                        self.current_use_idx += 1;
                    }
                }

                if is_promoted {
                    if let Some(var_name) = promoted_var {
                        *expr = HirExpr::Var(var_name);
                    }
                } else {
                    self.rewrite_expr(ptr);
                }
            }
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                self.rewrite_expr(expr);
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                self.rewrite_expr(lhs);
                self.rewrite_expr(rhs);
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.rewrite_expr(cond);
                self.rewrite_expr(then_expr);
                self.rewrite_expr(else_expr);
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    self.rewrite_expr(arg);
                }
            }
            HirExpr::Index { base, index, .. } => {
                self.rewrite_expr(base);
                self.rewrite_expr(index);
            }
            _ => {}
        }
    }
}

fn alias_key_for_ptr(ptr: &HirExpr, size: u32) -> AliasKey {
    let access_ty = NirType::Aggregate {
        size,
        fields: vec![],
    };
    partition_key_for_pointer_expr(ptr, &access_ty)
        .map(AliasKey::Partition)
        .unwrap_or(AliasKey::Unknown)
}

fn find_last_def_in_stmts(stmts: &[HirStmt], base_name: &str) -> Option<String> {
    for stmt in stmts.iter().rev() {
        match stmt {
            HirStmt::Assign { lhs, .. } => {
                if let HirLValue::Var(name) = lhs {
                    if name.starts_with(&format!("vVar_{}_", base_name)) {
                        return Some(name.clone());
                    }
                }
            }
            HirStmt::Block(body) => {
                if let Some(name) = find_last_def_in_stmts(body, base_name) {
                    return Some(name);
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if let Some(name) = find_last_def_in_stmts(then_body, base_name) {
                    return Some(name);
                }
                if let Some(name) = find_last_def_in_stmts(else_body, base_name) {
                    return Some(name);
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if let Some(name) = find_last_def_in_stmts(body, base_name) {
                    return Some(name);
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if let Some(name) = find_last_def_in_stmts(default, base_name) {
                    return Some(name);
                }
                for case in cases {
                    if let Some(name) = find_last_def_in_stmts(&case.body, base_name) {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn append_to_body_before_cf(body: &mut Vec<HirStmt>, stmt: HirStmt) {
    if let Some(last) = body.last() {
        if matches!(
            last,
            HirStmt::Break | HirStmt::Continue | HirStmt::Goto(_) | HirStmt::Return(_)
        ) {
            let idx = body.len() - 1;
            body.insert(idx, stmt);
            return;
        }
    }
    body.push(stmt);
}
