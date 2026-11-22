use super::constraints::UseConstraint;
use crate::prelude::*;
use crate::{HashMap, HashSet};

fn collect_copy_alias_sources(stmts: &[PreHirStmt], out: &mut HashMap<String, HashSet<String>>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } => {
                let mut source = rhs;
                while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = source {
                    source = expr.as_ref();
                }
                if let PreHirExpr::Var(source_name) = source {
                    out.entry(name.clone())
                        .or_default()
                        .insert(source_name.clone());
                }
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_copy_alias_sources(body, out);
            }
            PreHirStmt::DoWhile { body, .. } => collect_copy_alias_sources(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_alias_sources(then_body, out);
                collect_copy_alias_sources(else_body, out);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_copy_alias_sources(std::slice::from_ref(init), out);
                }
                if let Some(update) = update {
                    collect_copy_alias_sources(std::slice::from_ref(update), out);
                }
                collect_copy_alias_sources(body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_alias_sources(&case.body, out);
                }
                collect_copy_alias_sources(default, out);
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

pub(super) fn propagate_logical_shift_constraints_through_aliases(
    stmts: &[PreHirStmt],
    constraints: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let mut aliases = HashMap::default();
    collect_copy_alias_sources(stmts, &mut aliases);
    if aliases.is_empty() {
        return;
    }

    let mut work = Vec::new();
    for (name, items) in constraints.iter() {
        for item in items {
            if let UseConstraint::LogicalShiftUnsigned { bits } = item {
                work.push((name.clone(), *bits));
            }
        }
    }
    let mut seen = HashSet::default();
    while let Some((name, bits)) = work.pop() {
        if !seen.insert((name.clone(), bits)) {
            continue;
        }
        let Some(sources) = aliases.get(&name) else {
            continue;
        };
        for source in sources {
            constraints
                .entry(source.clone())
                .or_default()
                .push(UseConstraint::LogicalShiftUnsigned { bits });
            work.push((source.clone(), bits));
        }
    }
}
