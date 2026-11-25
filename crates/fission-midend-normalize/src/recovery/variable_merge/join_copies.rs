use super::merge_eligibility::{is_eligible_for_speculative_merge_by_name, name_priority};
use crate::HashMap;
use crate::prelude::*;

pub(super) fn collect_dominant_copy_join_merges(
    stmts: &[PreHirStmt],
    var_types: &HashMap<String, NirType>,
) -> Vec<(String, String)> {
    let mut renames = Vec::new();
    collect_dominant_copy_join_merges_in_stmts(stmts, var_types, &mut renames);
    renames
}

fn collect_dominant_copy_join_merges_in_stmts(
    stmts: &[PreHirStmt],
    var_types: &HashMap<String, NirType>,
    renames: &mut Vec<(String, String)>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::If {
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
                        let t1 = var_types.get(&dest1);
                        let t2 = var_types.get(&dest2);
                        if t1.is_none() || t2.is_none() || t1 == t2 {
                            let (keep, drop) = if name_priority(&dest1) >= name_priority(&dest2) {
                                (dest1, dest2)
                            } else {
                                (dest2, dest1)
                            };
                            renames.push((drop, keep));
                        }
                    }
                }
                collect_dominant_copy_join_merges_in_stmts(then_body, var_types, renames);
                collect_dominant_copy_join_merges_in_stmts(else_body, var_types, renames);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_dominant_copy_join_merges_in_stmts(body, var_types, renames);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_dominant_copy_join_merges_in_stmts(
                        std::slice::from_ref(init),
                        var_types,
                        renames,
                    );
                }
                if let Some(update) = update {
                    collect_dominant_copy_join_merges_in_stmts(
                        std::slice::from_ref(update),
                        var_types,
                        renames,
                    );
                }
                collect_dominant_copy_join_merges_in_stmts(body, var_types, renames);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_dominant_copy_join_merges_in_stmts(&case.body, var_types, renames);
                }
                collect_dominant_copy_join_merges_in_stmts(default, var_types, renames);
            }
            _ => {}
        }
    }
}

fn last_direct_var_copy(body: &[PreHirStmt]) -> Option<(String, String)> {
    body.iter().rev().find_map(|stmt| {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dest),
            rhs: PreHirExpr::Var(src),
        } = stmt
        {
            Some((dest.clone(), src.clone()))
        } else {
            None
        }
    })
}
