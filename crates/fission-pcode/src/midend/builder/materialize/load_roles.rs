use super::*;

impl<'a> PreviewBuilder<'a> {
    fn collect_var_names_in_expr(expr: &PreHirExpr, vars: &mut HashSet<String>) {
        match expr {
            PreHirExpr::Var(name) | PreHirExpr::AddressOfLocal(name) => {
                vars.insert(name.clone());
            }
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. } => {
                Self::collect_var_names_in_expr(expr, vars);
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                Self::collect_var_names_in_expr(lhs, vars);
                Self::collect_var_names_in_expr(rhs, vars);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    Self::collect_var_names_in_expr(arg, vars);
                }
            }
            PreHirExpr::Index { base, index, .. } => {
                Self::collect_var_names_in_expr(base, vars);
                Self::collect_var_names_in_expr(index, vars);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::collect_var_names_in_expr(cond, vars);
                Self::collect_var_names_in_expr(then_expr, vars);
                Self::collect_var_names_in_expr(else_expr, vars);
            }
            PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
        }
    }

    fn rhs_is_load_derived_value(&self, expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Load { .. } => true,
            PreHirExpr::Var(name) => self.load_value_bindings.contains(name),
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
                self.rhs_is_load_derived_value(expr)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.rhs_is_load_derived_value(lhs) || self.rhs_is_load_derived_value(rhs)
            }
            PreHirExpr::Call { args, .. } => {
                args.iter().any(|arg| self.rhs_is_load_derived_value(arg))
            }
            PreHirExpr::PtrOffset { base, .. }
            | PreHirExpr::FieldAccess { base, .. }
            | PreHirExpr::AggregateCopy { src: base, .. } => self.rhs_is_load_derived_value(base),
            PreHirExpr::Index { base, index, .. } => {
                self.rhs_is_load_derived_value(base) || self.rhs_is_load_derived_value(index)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.rhs_is_load_derived_value(cond)
                    || self.rhs_is_load_derived_value(then_expr)
                    || self.rhs_is_load_derived_value(else_expr)
            }
            PreHirExpr::Const(_, _)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => false,
        }
    }

    pub(super) fn record_load_value_roles(&mut self, lhs_name: &str, rhs: &PreHirExpr) {
        fn visit(this: &mut PreviewBuilder<'_>, expr: &PreHirExpr) {
            match expr {
                PreHirExpr::Load { ptr, .. } => {
                    let mut ptr_vars = HashSet::default();
                    PreviewBuilder::collect_var_names_in_expr(ptr, &mut ptr_vars);
                    this.load_address_bindings.extend(ptr_vars);
                    visit(this, ptr);
                }
                PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => visit(this, expr),
                PreHirExpr::Binary { lhs, rhs, .. } => {
                    visit(this, lhs);
                    visit(this, rhs);
                }
                PreHirExpr::Call { args, .. } => {
                    for arg in args {
                        visit(this, arg);
                    }
                }
                PreHirExpr::PtrOffset { base, .. }
                | PreHirExpr::FieldAccess { base, .. }
                | PreHirExpr::AggregateCopy { src: base, .. } => visit(this, base),
                PreHirExpr::Index { base, index, .. } => {
                    visit(this, base);
                    visit(this, index);
                }
                PreHirExpr::Select {
                    cond,
                    then_expr,
                    else_expr,
                    ..
                } => {
                    visit(this, cond);
                    visit(this, then_expr);
                    visit(this, else_expr);
                }
                PreHirExpr::Var(_)
                | PreHirExpr::Const(_, _)
                | PreHirExpr::AddressOfGlobal(_)
                | PreHirExpr::AddressOfLocal(_) => {}
            }
        }

        let is_load_derived = self.rhs_is_load_derived_value(rhs);
        visit(self, rhs);
        if is_load_derived {
            self.load_value_bindings.insert(lhs_name.to_string());
        }
    }

    pub(super) fn materialized_lhs_conflicts_with_load_address_role(
        &self,
        lhs_name: &str,
        rhs: &PreHirExpr,
    ) -> bool {
        self.load_address_bindings.contains(lhs_name)
            && self.rhs_is_load_derived_value(rhs)
            && !matches!(expr_type(rhs), NirType::Ptr(_))
    }
}
