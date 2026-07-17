//! Global symbol recovery before print (name aliases / pointer sugar).
//!
//! Mutates the structured tree for presentation of known globals only.
//! Does not perform semantic recovery beyond address→name rewrites.

use super::{HirExpr, HirFunction, HirLValue, HirStmt, MlilPreviewOptions};
use std::collections::HashMap;

pub(crate) fn recover_global_symbol_accesses(hir: &mut HirFunction, options: &MlilPreviewOptions) {
    let globals = global_symbol_name_map(hir, options);
    if globals.is_empty() {
        return;
    }
    let mut aliases = HashMap::new();
    recover_global_symbol_accesses_in_stmts(&mut hir.body, &globals, &mut aliases);
}

fn global_symbol_name_map(hir: &HirFunction, options: &MlilPreviewOptions) -> HashMap<u64, String> {
    options
        .global_names
        .iter()
        .filter(|(_, name)| is_c_identifier(name))
        .filter(|(_, name)| {
            name.as_str() != hir.name
                && !hir.params.iter().any(|binding| binding.name == **name)
                && !hir.locals.iter().any(|binding| binding.name == **name)
        })
        .map(|(addr, name)| (*addr, name.clone()))
        .collect()
}

fn recover_global_symbol_accesses_in_stmts(
    stmts: &mut [HirStmt],
    globals: &HashMap<u64, String>,
    aliases: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                recover_global_symbol_accesses_in_expr(rhs, globals, aliases);
                recover_global_symbol_accesses_in_lvalue(lhs, globals, aliases);
                if let HirLValue::Var(name) = lhs {
                    if let Some(global_name) = global_pointer_alias(rhs, globals) {
                        aliases.insert(name.clone(), global_name);
                    } else {
                        aliases.remove(name);
                    }
                }
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                recover_global_symbol_accesses_in_expr(expr, globals, aliases);
            }
            HirStmt::VaStart { va_list, .. } => {
                recover_global_symbol_accesses_in_expr(va_list, globals, aliases);
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                let mut nested_aliases = aliases.clone();
                recover_global_symbol_accesses_in_stmts(body, globals, &mut nested_aliases);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                recover_global_symbol_accesses_in_expr(expr, globals, aliases);
                for case in cases {
                    let mut case_aliases = aliases.clone();
                    recover_global_symbol_accesses_in_stmts(
                        &mut case.body,
                        globals,
                        &mut case_aliases,
                    );
                }
                let mut default_aliases = aliases.clone();
                recover_global_symbol_accesses_in_stmts(default, globals, &mut default_aliases);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                recover_global_symbol_accesses_in_expr(cond, globals, aliases);
                let mut then_aliases = aliases.clone();
                let mut else_aliases = aliases.clone();
                recover_global_symbol_accesses_in_stmts(then_body, globals, &mut then_aliases);
                recover_global_symbol_accesses_in_stmts(else_body, globals, &mut else_aliases);
                aliases.retain(|name, global| {
                    then_aliases
                        .get(name)
                        .is_some_and(|then_global| then_global == global)
                        && else_aliases
                            .get(name)
                            .is_some_and(|else_global| else_global == global)
                });
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init.as_mut() {
                    recover_global_symbol_accesses_in_stmts(
                        std::slice::from_mut(init),
                        globals,
                        aliases,
                    );
                }
                if let Some(cond) = cond {
                    recover_global_symbol_accesses_in_expr(cond, globals, aliases);
                }
                if let Some(update) = update.as_mut() {
                    let mut update_aliases = aliases.clone();
                    recover_global_symbol_accesses_in_stmts(
                        std::slice::from_mut(update),
                        globals,
                        &mut update_aliases,
                    );
                }
                let mut body_aliases = aliases.clone();
                recover_global_symbol_accesses_in_stmts(body, globals, &mut body_aliases);
            }
            HirStmt::Return(None)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn recover_global_symbol_accesses_in_lvalue(
    lvalue: &mut HirLValue,
    globals: &HashMap<u64, String>,
    aliases: &HashMap<String, String>,
) {
    match lvalue {
        HirLValue::Deref { ptr, .. } => {
            recover_global_symbol_accesses_in_expr(ptr, globals, aliases);
            if let Some(name) = global_pointer_alias(ptr, globals)
                .or_else(|| global_pointer_alias_from_expr(ptr, aliases))
            {
                *lvalue = HirLValue::Var(name);
            }
        }
        HirLValue::Index { base, index, .. } => {
            recover_global_symbol_accesses_in_expr(base, globals, aliases);
            recover_global_symbol_accesses_in_expr(index, globals, aliases);
        }
        HirLValue::Var(_) => {}
        HirLValue::FieldAccess { base, .. } => {
            recover_global_symbol_accesses_in_expr(base, globals, aliases);
        }
    }
}

fn recover_global_symbol_accesses_in_expr(
    expr: &mut HirExpr,
    globals: &HashMap<u64, String>,
    aliases: &HashMap<String, String>,
) {
    match expr {
        HirExpr::Load { ptr, .. } => {
            recover_global_symbol_accesses_in_expr(ptr, globals, aliases);
            if let Some(name) = global_pointer_alias(ptr, globals)
                .or_else(|| global_pointer_alias_from_expr(ptr, aliases))
            {
                *expr = HirExpr::Var(name);
            }
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            recover_global_symbol_accesses_in_expr(inner, globals, aliases);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            recover_global_symbol_accesses_in_expr(lhs, globals, aliases);
            recover_global_symbol_accesses_in_expr(rhs, globals, aliases);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            recover_global_symbol_accesses_in_expr(cond, globals, aliases);
            recover_global_symbol_accesses_in_expr(then_expr, globals, aliases);
            recover_global_symbol_accesses_in_expr(else_expr, globals, aliases);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                recover_global_symbol_accesses_in_expr(arg, globals, aliases);
            }
        }
        HirExpr::Index { base, index, .. } => {
            recover_global_symbol_accesses_in_expr(base, globals, aliases);
            recover_global_symbol_accesses_in_expr(index, globals, aliases);
        }
        HirExpr::Var(name) => {
            if let Some(global_name) = aliases.get(name) {
                *expr = HirExpr::AddressOfGlobal(global_name.clone());
            }
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn global_pointer_alias(expr: &HirExpr, globals: &HashMap<u64, String>) -> Option<String> {
    match expr {
        HirExpr::Const(value, _) => u64::try_from(*value)
            .ok()
            .and_then(|addr| globals.get(&addr).cloned()),
        HirExpr::Cast { expr, .. }
        | HirExpr::PtrOffset {
            base: expr,
            offset: 0,
        } => global_pointer_alias(expr, globals),
        HirExpr::AddressOfGlobal(name) => Some(name.clone()),
        _ => None,
    }
}

fn global_pointer_alias_from_expr(
    expr: &HirExpr,
    aliases: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        HirExpr::Var(name) => aliases.get(name).cloned(),
        HirExpr::Cast { expr, .. }
        | HirExpr::PtrOffset {
            base: expr,
            offset: 0,
        } => global_pointer_alias_from_expr(expr, aliases),
        HirExpr::AddressOfGlobal(name) => Some(name.clone()),
        _ => None,
    }
}

pub(crate) fn is_c_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

