use crate::prelude::*;

pub fn apply_split_datatype_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;
    let mut new_body = Vec::new();
    for stmt in func.body.drain(..) {
        let mut mutated = stmt.clone();
        if recurse_split_stmt(&mut mutated) {
            changed = true;
        }
        if let Some(split_stmts) = try_split_stmt(&mutated) {
            new_body.extend(split_stmts);
            changed = true;
        } else {
            new_body.push(mutated);
        }
    }
    func.body = new_body;
    changed
}

fn recurse_split_stmt(stmt: &mut DirStmt) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Block(body) => {
            changed |= split_datatype_in_stmts(body);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= split_datatype_in_stmts(body);
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                if let DirStmt::Block(body) = init.as_mut() {
                    changed |= split_datatype_in_stmts(body);
                }
            }
            if let Some(update) = update {
                if let DirStmt::Block(body) = update.as_mut() {
                    changed |= split_datatype_in_stmts(body);
                }
            }
            changed |= split_datatype_in_stmts(body);
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            changed |= split_datatype_in_stmts(then_body);
            changed |= split_datatype_in_stmts(else_body);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                changed |= split_datatype_in_stmts(&mut case.body);
            }
            changed |= split_datatype_in_stmts(default);
        }
        _ => {}
    }
    changed
}

fn split_datatype_in_stmts(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut new_stmts = Vec::new();
    for mut stmt in stmts.drain(..) {
        changed |= recurse_split_stmt(&mut stmt);
        if let Some(split) = try_split_stmt(&stmt) {
            new_stmts.extend(split);
            changed = true;
        } else {
            new_stmts.push(stmt);
        }
    }
    *stmts = new_stmts;
    changed
}

fn try_split_stmt(stmt: &DirStmt) -> Option<Vec<DirStmt>> {
    let DirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let DirLValue::Deref {
        ptr: dest,
        ty: NirType::Aggregate { fields, .. },
    } = lhs
    else {
        return None;
    };
    if fields.is_empty() {
        return None;
    }

    match rhs {
        DirExpr::Load {
            ptr: src,
            ty: NirType::Aggregate { .. },
        } => {
            let mut split = Vec::new();
            for field in fields {
                let new_lhs = DirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = DirExpr::Load {
                    ptr: Box::new(make_ptr_offset((**src).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                split.push(DirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
            }
            Some(split)
        }
        DirExpr::AggregateCopy { src, .. } => {
            let mut split = Vec::new();
            for field in fields {
                let new_lhs = DirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = DirExpr::Load {
                    ptr: Box::new(make_ptr_offset((**src).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                split.push(DirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
            }
            Some(split)
        }
        DirExpr::Const(0, _) => {
            let mut split = Vec::new();
            for field in fields {
                let new_lhs = DirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = DirExpr::Const(0, field.ty.clone());
                split.push(DirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
            }
            Some(split)
        }
        _ => None,
    }
}

fn make_ptr_offset(ptr: DirExpr, offset: i64) -> DirExpr {
    match ptr {
        DirExpr::PtrOffset {
            base,
            offset: existing_offset,
        } => DirExpr::PtrOffset {
            base,
            offset: existing_offset + offset,
        },
        _ => DirExpr::PtrOffset {
            base: Box::new(ptr),
            offset,
        },
    }
}
