use super::merge_eligibility::{is_eligible_for_speculative_merge_by_name, name_priority};
use crate::prelude::*;
use crate::{HashMap, HashSet};

pub(super) fn collect_direct_copies(stmts: &[PreHirStmt]) -> crate::HashSet<(String, String)> {
    let mut copies = crate::HashSet::default();
    // Only *unconditional* var copies establish merge identity. A copy inside
    // `if`/`while`/switch (e.g. x86 cmov `if (cond) acc = src`) is a
    // path-sensitive override: merging `acc` with `src` collapses distinct
    // values (hi default vs lo source) and destroys clamp/min/max chains.
    fn visit(
        stmts: &[PreHirStmt],
        copies: &mut crate::HashSet<(String, String)>,
        under_control: bool,
    ) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(lhs_name),
                    rhs: PreHirExpr::Var(rhs_name),
                } if !under_control => {
                    copies.insert((lhs_name.clone(), rhs_name.clone()));
                    copies.insert((rhs_name.clone(), lhs_name.clone()));
                }
                PreHirStmt::Block(body) => {
                    visit(body, copies, under_control);
                }
                PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                    visit(body, copies, true);
                }
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    visit(then_body, copies, true);
                    visit(else_body, copies, true);
                }
                PreHirStmt::For {
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
                PreHirStmt::Switch { cases, default, .. } => {
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

pub(super) fn sorted_var_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

pub(super) fn transitive_copy_aliases(
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
