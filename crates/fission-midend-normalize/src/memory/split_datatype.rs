use crate::global_opt::nir_byte_size;
use crate::prelude::*;

pub fn apply_split_datatype_pass(func: &mut PreHirFunction) -> bool {
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

fn recurse_split_stmt(stmt: &mut PreHirStmt) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Block(body) => {
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                if let PreHirStmt::Block(body) = init.as_mut() {
                    changed |=
                        split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
            }
            if let Some(update) = update {
                if let PreHirStmt::Block(body) = update.as_mut() {
                    changed |=
                        split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
            }
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                    &mut case.body,
                ));
            }
            changed |= split_datatype_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
        }
        _ => {}
    }
    changed
}

fn split_datatype_in_stmts(stmts: &mut Vec<PreHirStmt>) -> bool {
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

fn try_split_stmt(stmt: &PreHirStmt) -> Option<Vec<PreHirStmt>> {
    let PreHirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let PreHirLValue::Deref {
        ptr: dest,
        ty: NirType::Aggregate { size, fields },
    } = lhs
    else {
        return None;
    };
    if fields.is_empty() {
        return None;
    }
    if fields.iter().any(|f| matches!(f.ty, NirType::Unknown)) {
        return None;
    }

    match rhs {
        PreHirExpr::Load {
            ptr: src,
            ty: NirType::Aggregate { .. },
        } => {
            let mut split = Vec::new();
            for field in fields {
                let new_lhs = PreHirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = PreHirExpr::Load {
                    ptr: Box::new(make_ptr_offset((**src).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                split.push(PreHirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
            }
            Some(split)
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            let mut split = Vec::new();
            for field in fields {
                let new_lhs = PreHirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = PreHirExpr::Load {
                    ptr: Box::new(make_ptr_offset((**src).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                split.push(PreHirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
            }
            Some(split)
        }
        PreHirExpr::Const(0, _) => {
            let mut split = Vec::new();
            let mut current_offset = 0u32;
            for field in fields {
                if field.offset > current_offset {
                    emit_zero_fill(dest, current_offset, field.offset, &mut split);
                }
                let f_size = nir_byte_size(&field.ty);
                let new_lhs = PreHirLValue::Deref {
                    ptr: Box::new(make_ptr_offset((**dest).clone(), field.offset as i64)),
                    ty: field.ty.clone(),
                };
                let new_rhs = PreHirExpr::Const(0, field.ty.clone());
                split.push(PreHirStmt::Assign {
                    lhs: new_lhs,
                    rhs: new_rhs,
                });
                current_offset = (field.offset + f_size).max(current_offset);
            }
            if *size > current_offset {
                emit_zero_fill(dest, current_offset, *size, &mut split);
            }
            Some(split)
        }
        _ => None,
    }
}

fn emit_zero_fill(
    dest: &PreHirExpr,
    start_offset: u32,
    end_offset: u32,
    split: &mut Vec<PreHirStmt>,
) {
    let mut cur = start_offset;
    while cur < end_offset {
        let rem = end_offset - cur;
        let (ty, step) = if rem >= 8 && cur % 8 == 0 {
            (
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
                8,
            )
        } else if rem >= 4 && cur % 4 == 0 {
            (
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
                4,
            )
        } else if rem >= 2 && cur % 2 == 0 {
            (
                NirType::Int {
                    bits: 16,
                    signed: false,
                },
                2,
            )
        } else {
            (
                NirType::Int {
                    bits: 8,
                    signed: false,
                },
                1,
            )
        };
        split.push(PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(make_ptr_offset(dest.clone(), cur as i64)),
                ty: ty.clone(),
            },
            rhs: PreHirExpr::Const(0, ty),
        });
        cur += step;
    }
}

fn make_ptr_offset(ptr: PreHirExpr, offset: i64) -> PreHirExpr {
    match ptr {
        PreHirExpr::PtrOffset {
            base,
            offset: existing_offset,
        } => PreHirExpr::PtrOffset {
            base,
            offset: existing_offset + offset,
        },
        _ => PreHirExpr::PtrOffset {
            base: Box::new(ptr),
            offset,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_const_split_fills_padding_gaps() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };

        let agg_ty = NirType::Aggregate {
            size: 16,
            fields: vec![
                fission_midend_core::StructField {
                    offset: 0,
                    ty: u32_ty.clone(),
                    name: "a".to_string(),
                },
                fission_midend_core::StructField {
                    offset: 8,
                    ty: u64_ty.clone(),
                    name: "b".to_string(),
                },
            ],
        };

        let mut func = PreHirFunction {
            name: "test_zero".to_string(),
            is_64bit: true,
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("p".to_string())),
                    ty: agg_ty,
                },
                rhs: PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                ),
            }],
            ..Default::default()
        };

        assert!(apply_split_datatype_pass(&mut func));
        // Should have 3 writes: field 0 (4 bytes), gap 4..8 (4 bytes), field 1 (8 bytes)
        assert_eq!(func.body.len(), 3);
        match &func.body[0] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, ty },
                ..
            } => {
                assert_eq!(*ty, u32_ty);
                assert!(matches!(
                    ptr.as_ref(),
                    PreHirExpr::PtrOffset { offset: 0, .. }
                ));
            }
            _ => panic!("expected Deref assign"),
        }
        match &func.body[1] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, ty },
                ..
            } => {
                assert_eq!(*ty, u32_ty);
                assert!(matches!(
                    ptr.as_ref(),
                    PreHirExpr::PtrOffset { offset: 4, .. }
                ));
            }
            _ => panic!("expected gap zero fill assign"),
        }
        match &func.body[2] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, ty },
                ..
            } => {
                assert_eq!(*ty, u64_ty);
                assert!(matches!(
                    ptr.as_ref(),
                    PreHirExpr::PtrOffset { offset: 8, .. }
                ));
            }
            _ => panic!("expected Deref assign"),
        }
    }

    #[test]
    fn rejects_splitting_unknown_field_type() {
        let agg_ty = NirType::Aggregate {
            size: 4,
            fields: vec![fission_midend_core::StructField {
                offset: 0,
                ty: NirType::Unknown,
                name: "f0".to_string(),
            }],
        };

        let mut func = PreHirFunction {
            name: "test_unknown".to_string(),
            is_64bit: true,
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("p".to_string())),
                    ty: agg_ty,
                },
                rhs: PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            }],
            ..Default::default()
        };

        assert!(!apply_split_datatype_pass(&mut func));
    }
}
