use super::super::global_opt::{AliasKey, MemDef, MemPhi, MemUse, build_mem_ssa, nir_byte_size};
use super::partition_key_for_pointer_expr;
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Incremental Memory Heritage Solver pass (Ghidra `Heritage` partial).
///
/// Promotes eligible memory locations (non-escaping fixed PartitionKeys across
/// stack/aggregate spaces) to versioned virtual SSA variables in `func.locals`,
/// replacing load/store ops with direct variable accesses and inserting
/// phi-assignments at block merges.
///
/// Type witness: store/load access types per AliasKey (Float > Ptr > Int >
/// Aggregate) so heritage vars inherit real types rather than default uint.
/// HighVariable `Cover` merge of resulting `vVar_*` versions is owned by
/// [`crate::recovery::variable_merge::apply_variable_merge_pass`].
pub fn apply_memory_heritage(func: &mut PreHirFunction) -> bool {
    let mem_ssa = build_mem_ssa(func);

    // All heritage-promotable spaces: fixed non-escaping stack/aggregate partitions.
    let mut promotable_keys = HashSet::default();
    for def in &mem_ssa.defs {
        if !def.may_escape {
            if let AliasKey::Partition(partition) = &def.key {
                if partition.is_heritage_promotable() {
                    promotable_keys.insert(def.key.clone());
                }
            }
        }
    }

    if promotable_keys.is_empty() {
        return false;
    }

    // Cover-inspired type witness: prefer most specific access type per key.
    let access_ty_by_key = collect_access_types_for_keys(func, &promotable_keys);

    // Allocate versioned variable names for each def/phi ID of promotable keys
    let mut var_names = HashMap::default(); // maps (AliasKey, id) -> variable name String
    let mut var_types = HashMap::default();

    for def in &mem_ssa.defs {
        if promotable_keys.contains(&def.key) {
            let AliasKey::Partition(partition) = &def.key else {
                continue;
            };
            let size = (partition.offset_interval.1 - partition.offset_interval.0).max(1) as u32;
            let ty = access_ty_by_key
                .get(&def.key)
                .cloned()
                .unwrap_or(NirType::Int {
                    bits: size * 8,
                    signed: false,
                });
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
            let ty = access_ty_by_key
                .get(&phi.key)
                .cloned()
                .unwrap_or(NirType::Int {
                    bits: size * 8,
                    signed: false,
                });
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
        new_locals.push(PreHirBinding {
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

/// Prefer more specific access types when multiple witnesses exist for one key.
fn type_specificity_rank(ty: &NirType) -> u8 {
    match ty {
        NirType::Float { .. } => 4,
        NirType::Ptr(_) => 3,
        NirType::Int { .. } | NirType::Bool => 2,
        NirType::Aggregate { .. } => 1,
        NirType::Unknown => 0,
    }
}

fn record_access_type(out: &mut HashMap<AliasKey, NirType>, key: AliasKey, ty: &NirType) {
    match out.get(&key) {
        Some(existing) if type_specificity_rank(existing) >= type_specificity_rank(ty) => {}
        _ => {
            out.insert(key, ty.clone());
        }
    }
}

/// Scan the function for typed memory accesses on promotable keys
/// (Cover-inspired type witness for heritage vars across all type spaces).
fn collect_access_types_for_keys(
    func: &PreHirFunction,
    promotable: &HashSet<AliasKey>,
) -> HashMap<AliasKey, NirType> {
    let mut out = HashMap::default();
    collect_access_types_in_stmts(&func.body, promotable, &mut out);
    out
}

fn collect_access_types_in_stmts(
    stmts: &[PreHirStmt],
    promotable: &HashSet<AliasKey>,
    out: &mut HashMap<AliasKey, NirType>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Deref { ptr, ty } = lhs {
                    if !matches!(ty, NirType::Unknown) {
                        let key = alias_key_for_ptr(ptr, nir_byte_size(ty));
                        if promotable.contains(&key) {
                            record_access_type(out, key, ty);
                        }
                    }
                }
                if let PreHirLValue::Index { base, elem_ty, .. } = lhs {
                    if !matches!(elem_ty, NirType::Unknown) {
                        let key = alias_key_for_ptr(base, nir_byte_size(elem_ty));
                        if promotable.contains(&key) {
                            record_access_type(out, key, elem_ty);
                        }
                    }
                }
                collect_access_types_in_expr(rhs, promotable, out);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_access_types_in_stmts(body, promotable, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_access_types_in_stmts(then_body, promotable, out);
                collect_access_types_in_stmts(else_body, promotable, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_access_types_in_stmts(&case.body, promotable, out);
                }
                collect_access_types_in_stmts(default, promotable, out);
            }
            PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => {
                collect_access_types_in_expr(e, promotable, out);
            }
            _ => {}
        }
    }
}

fn collect_access_types_in_expr(
    expr: &PreHirExpr,
    promotable: &HashSet<AliasKey>,
    out: &mut HashMap<AliasKey, NirType>,
) {
    match expr {
        PreHirExpr::Load { ptr, ty } if !matches!(ty, NirType::Unknown) => {
            let key = alias_key_for_ptr(ptr, nir_byte_size(ty));
            if promotable.contains(&key) {
                record_access_type(out, key, ty);
            }
            collect_access_types_in_expr(ptr, promotable, out);
        }
        PreHirExpr::Index {
            base,
            elem_ty,
            index,
        } if !matches!(elem_ty, NirType::Unknown) => {
            let key = alias_key_for_ptr(base, nir_byte_size(elem_ty));
            if promotable.contains(&key) {
                record_access_type(out, key, elem_ty);
            }
            collect_access_types_in_expr(base, promotable, out);
            collect_access_types_in_expr(index, promotable, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_access_types_in_expr(lhs, promotable, out);
            collect_access_types_in_expr(rhs, promotable, out);
        }
        PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Cast { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            collect_access_types_in_expr(expr, promotable, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_access_types_in_expr(cond, promotable, out);
            collect_access_types_in_expr(then_expr, promotable, out);
            collect_access_types_in_expr(else_expr, promotable, out);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args {
                collect_access_types_in_expr(a, promotable, out);
            }
        }
        PreHirExpr::Load { ptr, .. } => collect_access_types_in_expr(ptr, promotable, out),
        PreHirExpr::Index { base, index, .. } => {
            collect_access_types_in_expr(base, promotable, out);
            collect_access_types_in_expr(index, promotable, out);
        }
        _ => {}
    }
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
    fn rewrite_stmts(&mut self, stmts: &mut Vec<PreHirStmt>) {
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

    fn rewrite_stmt(&mut self, stmt: &mut PreHirStmt) -> Vec<PreHirStmt> {
        let mut pre_insert = Vec::new();
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                // Rewrite rhs uses first
                self.rewrite_expr(rhs);

                // Rewrite lhs store if it matches a promotable slot
                let mut is_promoted = false;
                let mut new_var_name = None;

                match lhs {
                    PreHirLValue::Deref { ptr, ty } => {
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
                    PreHirLValue::Index {
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
                        *lhs = PreHirLValue::Var(var_name);
                    }
                }
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                self.rewrite_expr(expr);
            }
            PreHirStmt::Block(body) => {
                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.rewrite_expr(cond);

                // Rewrite then/else branches
                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));

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
                                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                                    PreHirStmt::Assign {
                                        lhs: PreHirLValue::Var(phi_var.clone()),
                                        rhs: PreHirExpr::Var(then_var.clone()),
                                    },
                                );
                            }
                            // else branch assignment: phi_var = else_input_var
                            if let Some(else_var) =
                                self.var_names.get(&(phi.key.clone(), else_input))
                            {
                                append_to_body_before_cf(
                                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                                    PreHirStmt::Assign {
                                        lhs: PreHirLValue::Var(phi_var.clone()),
                                        rhs: PreHirExpr::Var(else_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            PreHirStmt::While { cond, body } => {
                self.rewrite_expr(cond);

                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));

                // Insert phi-initializations before the loop, and loop-carried updates at the end of body.
                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0]; // loop body end
                        let pre_input = phi.inputs[1]; // before loop

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            // Pre-loop: phi_var = pre_input_var
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(PreHirStmt::Assign {
                                    lhs: PreHirLValue::Var(phi_var.clone()),
                                    rhs: PreHirExpr::Var(pre_var.clone()),
                                });
                            }
                            // Loop end: phi_var = body_input_var
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                                    PreHirStmt::Assign {
                                        lhs: PreHirLValue::Var(phi_var.clone()),
                                        rhs: PreHirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            PreHirStmt::DoWhile { body, cond } => {
                let mut merge_phis = Vec::new();
                while self.current_phi_idx < self.phis.len() {
                    let phi = &self.phis[self.current_phi_idx];
                    if self.promotable_keys.contains(&phi.key) {
                        merge_phis.push(phi.clone());
                    }
                    self.current_phi_idx += 1;
                }

                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                self.rewrite_expr(cond);

                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0];
                        let pre_input = phi.inputs[1];

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(PreHirStmt::Assign {
                                    lhs: PreHirLValue::Var(phi_var.clone()),
                                    rhs: PreHirExpr::Var(pre_var.clone()),
                                });
                            }
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                                    PreHirStmt::Assign {
                                        lhs: PreHirLValue::Var(phi_var.clone()),
                                        rhs: PreHirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            PreHirStmt::For {
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
                        *s = Box::new(PreHirStmt::Block(dummy.into()));
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

                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));

                if let Some(s) = update {
                    let mut dummy = vec![(**s).clone()];
                    self.rewrite_stmts(&mut dummy);
                    if dummy.len() == 1 {
                        *s = Box::new(dummy.remove(0));
                    } else if !dummy.is_empty() {
                        *s = Box::new(PreHirStmt::Block(dummy.into()));
                    }
                }

                for phi in merge_phis {
                    if phi.inputs.len() >= 2 {
                        let body_input = phi.inputs[0];
                        let pre_input = phi.inputs[1];

                        if let Some(phi_var) = self.var_names.get(&(phi.key.clone(), phi.id)) {
                            if let Some(pre_var) = self.var_names.get(&(phi.key.clone(), pre_input))
                            {
                                pre_insert.push(PreHirStmt::Assign {
                                    lhs: PreHirLValue::Var(phi_var.clone()),
                                    rhs: PreHirExpr::Var(pre_var.clone()),
                                });
                            }
                            if let Some(body_var) =
                                self.var_names.get(&(phi.key.clone(), body_input))
                            {
                                append_to_body_before_cf(
                                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                                    PreHirStmt::Assign {
                                        lhs: PreHirLValue::Var(phi_var.clone()),
                                        rhs: PreHirExpr::Var(body_var.clone()),
                                    },
                                );
                            }
                        }
                    }
                }
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.rewrite_expr(expr);

                for case in cases.iter_mut() {
                    self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
                }
                self.rewrite_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));

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
                let mut phis_by_key: HashMap<AliasKey, Vec<MemPhi>> = HashMap::default();
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
                                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                                                &mut case.body,
                                            ),
                                            PreHirStmt::Assign {
                                                lhs: PreHirLValue::Var(phi_var.clone()),
                                                rhs: PreHirExpr::Var(v),
                                            },
                                        );
                                    }
                                }
                                // Assign for default arm
                                let default_var = find_last_def_in_stmts(default, &base_name)
                                    .or_else(|| pre_var.cloned());
                                if let Some(v) = default_var {
                                    append_to_body_before_cf(
                                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                                        PreHirStmt::Assign {
                                            lhs: PreHirLValue::Var(phi_var.clone()),
                                            rhs: PreHirExpr::Var(v),
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

    fn rewrite_expr(&mut self, expr: &mut PreHirExpr) {
        match expr {
            PreHirExpr::Load { ptr, ty } => {
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
                        *expr = PreHirExpr::Var(var_name);
                    }
                } else {
                    self.rewrite_expr(ptr);
                }
            }
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. } => {
                self.rewrite_expr(expr);
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.rewrite_expr(lhs);
                self.rewrite_expr(rhs);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.rewrite_expr(cond);
                self.rewrite_expr(then_expr);
                self.rewrite_expr(else_expr);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    self.rewrite_expr(arg);
                }
            }
            PreHirExpr::Index { base, index, .. } => {
                self.rewrite_expr(base);
                self.rewrite_expr(index);
            }
            _ => {}
        }
    }
}

fn alias_key_for_ptr(ptr: &PreHirExpr, size: u32) -> AliasKey {
    let access_ty = NirType::Aggregate {
        size,
        fields: vec![],
    };
    partition_key_for_pointer_expr(ptr, &access_ty)
        .map(AliasKey::Partition)
        .unwrap_or(AliasKey::Unknown)
}

fn find_last_def_in_stmts(stmts: &[PreHirStmt], base_name: &str) -> Option<String> {
    for stmt in stmts.iter().rev() {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => {
                if let PreHirLValue::Var(name) = lhs {
                    if name.starts_with(&format!("vVar_{}_", base_name)) {
                        return Some(name.clone());
                    }
                }
            }
            PreHirStmt::Block(body) => {
                if let Some(name) = find_last_def_in_stmts(body, base_name) {
                    return Some(name);
                }
            }
            PreHirStmt::If {
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
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                if let Some(name) = find_last_def_in_stmts(body, base_name) {
                    return Some(name);
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
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

fn append_to_body_before_cf(body: &mut Vec<PreHirStmt>, stmt: PreHirStmt) {
    if let Some(last) = body.last() {
        if matches!(
            last,
            PreHirStmt::Break | PreHirStmt::Continue | PreHirStmt::Goto(_) | PreHirStmt::Return(_)
        ) {
            let idx = body.len() - 1;
            body.insert(idx, stmt);
            return;
        }
    }
    body.push(stmt);
}
