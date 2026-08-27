//! DREAM emission: turn reaching conditions back into nested structure.
//!
//! [`crate::reaching_conditions`] answers *when* each node runs. This module
//! answers *how to write that down* -- and it never writes a jump, because
//! there is no case where it fails to find one.
//!
//! The emission itself is almost trivial: visit nodes in topological order and
//! guard each one by its reaching condition. That is already correct, since in
//! an acyclic single-entry region every node runs at most once and the
//! topological order respects every dependency. What it is not is *readable*:
//! it produces a flat run of `if`s with the same sub-conditions written out
//! over and over.
//!
//! So the work here is factoring. Given
//!
//! ```text
//! A under  a
//! B under  a AND b
//! C under  a AND NOT b
//! ```
//!
//! the flat form repeats `a` three times; the factored form is
//!
//! ```text
//! if (a) { A; if (b) { B } else { C } }
//! ```
//!
//! [`structure_guarded_entries`] does that recursively, splitting each run on
//! a shared conjunct.
//!
//! # The soundness obligation
//!
//! A reaching condition is a formula over *branch conditions taken from other
//! blocks*. Emitting it at a node's position re-evaluates those expressions
//! there -- after assignments that may have changed what they read. A raw
//! `*ptr < 10` hoisted past a store to `ptr` is simply a different program.
//!
//! This is not hypothetical for Fission: the short-circuit condition folding
//! attempted earlier in this work failed 12,794 times with **96.4% of the
//! rejections blocked by exactly this**, un-hoistable `Load`s.
//!
//! [`materialize_branch_conditions`] is the answer, and it is why this
//! approach survives where hoisting did not. Each branch condition is bound
//! to a fresh boolean at the point the branch originally happened, and the
//! reaching conditions are built over *those variables*. A variable is pure by
//! construction, so no guard can ever be invalidated by what runs between its
//! definition and its use. Callers that skip this step and feed raw
//! expressions in are responsible for their purity.
//!
//! Pure and host-free like its sibling: bodies and names arrive through
//! closures, so nothing here can lower a statement or touch builder state.

use crate::HashMap;
use crate::reaching_conditions::{
    NodeId, always, conditions_are_complementary, conjunct_refs, conjunction, is_always,
};
use fission_midend_prehir::util::negate_expr;
use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt};

/// A node's statements together with the condition they run under.
#[derive(Debug, Clone)]
pub struct GuardedEntry {
    pub cond: PreHirExpr,
    pub stmts: Vec<PreHirStmt>,
}

/// Emit an acyclic region as nested conditionals, with no jumps.
///
/// `order` must be a topological order of the region and `reaching` the
/// conditions computed for it. Nodes missing from `reaching` are unreachable
/// and contribute nothing.
pub fn emit_acyclic_region(
    order: &[NodeId],
    reaching: &HashMap<NodeId, PreHirExpr>,
    node_body: impl Fn(NodeId) -> Vec<PreHirStmt>,
) -> Vec<PreHirStmt> {
    let mut entries: Vec<GuardedEntry> = Vec::new();
    for &n in order {
        let Some(cond) = reaching.get(&n) else {
            continue;
        };
        let stmts = node_body(n);
        if stmts.is_empty() {
            continue;
        }
        // A node that runs under the same guard as the one before it belongs
        // in the same `if`, not a second copy of it.
        match entries.last_mut() {
            Some(prev) if &prev.cond == cond => prev.stmts.extend(stmts),
            _ => entries.push(GuardedEntry {
                cond: cond.clone(),
                stmts,
            }),
        }
    }
    structure_guarded_entries(&entries)
}

/// Factor a run of guarded entries into nested conditionals.
///
/// Recursive: pick the first conjunct of the first guarded entry, take the
/// maximal run of entries that mention it either way, and split that run into
/// the two arms of one `if` with the conjunct removed from each.
pub fn structure_guarded_entries(entries: &[GuardedEntry]) -> Vec<PreHirStmt> {
    let mut out: Vec<PreHirStmt> = Vec::new();
    let mut i = 0usize;
    while i < entries.len() {
        let entry = &entries[i];
        if is_always(&entry.cond) {
            out.extend(entry.stmts.iter().cloned());
            i += 1;
            continue;
        }
        let Some(split) = conjunct_refs(&entry.cond).first().map(|c| (*c).clone()) else {
            i += 1;
            continue;
        };
        let negated = negate_expr(split.clone());

        // Extend while entries keep deciding on the same conjunct. Stopping at
        // the first that does not is what keeps side effects in order across
        // the boundary.
        let mut end = i;
        while end < entries.len()
            && (has_conjunct(&entries[end].cond, &split)
                || has_conjunct(&entries[end].cond, &negated))
        {
            end += 1;
        }

        let mut then_arm: Vec<GuardedEntry> = Vec::new();
        let mut else_arm: Vec<GuardedEntry> = Vec::new();
        for e in &entries[i..end] {
            if has_conjunct(&e.cond, &split) {
                then_arm.push(GuardedEntry {
                    cond: strip_conjunct(&e.cond, &split),
                    stmts: e.stmts.clone(),
                });
            } else {
                else_arm.push(GuardedEntry {
                    cond: strip_conjunct(&e.cond, &negated),
                    stmts: e.stmts.clone(),
                });
            }
        }

        // Reordering across the arms is safe precisely because they are
        // exclusive: `split` and its negation never both hold, so no two
        // statements that could both run have swapped places.
        let then_body = structure_guarded_entries(&then_arm);
        let else_body = structure_guarded_entries(&else_arm);
        push_if(&mut out, split, then_body, else_body);
        i = end;
    }
    out
}

/// Emit one `if`, inverting when that avoids an empty then-arm.
fn push_if(
    out: &mut Vec<PreHirStmt>,
    cond: PreHirExpr,
    then_body: Vec<PreHirStmt>,
    else_body: Vec<PreHirStmt>,
) {
    if then_body.is_empty() && else_body.is_empty() {
        return;
    }
    // `if (c) {} else { X }` reads as `if (!c) { X }`. The empty-shell form is
    // what Fission's HIR presentation invariants reject, so never emit it.
    let (cond, then_body, else_body) = if then_body.is_empty() {
        (negate_expr(cond), else_body, Vec::new())
    } else {
        (cond, then_body, else_body)
    };
    out.push(PreHirStmt::If {
        cond,
        then_body: std::rc::Rc::new(then_body),
        else_body: std::rc::Rc::new(else_body),
    });
}

fn has_conjunct(e: &PreHirExpr, needle: &PreHirExpr) -> bool {
    conjunct_refs(e).iter().any(|c| *c == needle)
}

/// Remove `needle` from a conjunction; `true` once nothing is left.
fn strip_conjunct(e: &PreHirExpr, needle: &PreHirExpr) -> PreHirExpr {
    conjunction(
        conjunct_refs(e)
            .into_iter()
            .filter(|c| *c != needle)
            .cloned()
            .collect(),
    )
}

/// Fold a guard binding into the `if` that immediately consumes it.
///
/// [`materialize_branch_conditions`] binds every decision so that a guard is
/// safe to evaluate wherever it ends up. Most guards do not actually travel:
/// after factoring, the `if` that uses one sits directly after its binding,
/// and the variable is then pure overhead. Measured on a two-block `if`, the
/// unfolded form is `dream_c0 = !param_1; if (!dream_c0)` where the existing
/// path writes `if (param_1)` -- and the variable is not even declared, since
/// nothing registered it with the builder.
///
/// A binding is inlined only when it is read exactly once. That is decided by
/// counting the name in the debug rendering of the whole body rather than by
/// a hand-written walk over the expression variants: a variant missed in such
/// a walk would under-count and inline a guard still read elsewhere, while an
/// extra mention here can only make this decline.
pub fn inline_single_use_guards(
    body: Vec<PreHirStmt>,
    is_guard: impl Fn(&str) -> bool + Copy,
) -> Vec<PreHirStmt> {
    let rendered = format!("{body:?}");
    // One mention is the binding's own left-hand side, so a single read is two.
    let single_use = |name: &str| rendered.matches(&format!("{name:?}")).count() == 2;
    inline_in(body, is_guard, &single_use)
}

fn inline_in(
    body: Vec<PreHirStmt>,
    is_guard: impl Fn(&str) -> bool + Copy,
    single_use: &impl Fn(&str) -> bool,
) -> Vec<PreHirStmt> {
    let mut out: Vec<PreHirStmt> = Vec::with_capacity(body.len());
    let mut iter = body.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        // Recurse first so nested bodies are folded too.
        let stmt = match stmt {
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(inline_in(
                    then_body.as_ref().clone(),
                    is_guard,
                    single_use,
                )),
                else_body: std::rc::Rc::new(inline_in(
                    else_body.as_ref().clone(),
                    is_guard,
                    single_use,
                )),
            },
            other => other,
        };

        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = &stmt
        else {
            out.push(stmt);
            continue;
        };
        if !is_guard(name) || !single_use(name) {
            out.push(stmt);
            continue;
        }
        let var = PreHirExpr::Var(name.clone());
        let folded = match iter.peek() {
            Some(PreHirStmt::If { cond, .. }) if cond == &var => Some(rhs.clone()),
            Some(PreHirStmt::If { cond, .. }) if cond == &negate_expr(var.clone()) => {
                Some(negate_expr(rhs.clone()))
            }
            _ => None,
        };
        let Some(cond) = folded else {
            out.push(stmt);
            continue;
        };
        let Some(PreHirStmt::If {
            then_body,
            else_body,
            ..
        }) = iter.next()
        else {
            unreachable!("peeked an if")
        };
        out.push(PreHirStmt::If {
            cond,
            then_body,
            else_body,
        });
    }
    out
}

/// Whether any guard binding survived inlining.
///
/// A guard that is read more than once cannot be folded away, and this module
/// cannot leave it standing either: the name was invented here, never
/// registered with the builder, so it would print undeclared. Callers decline
/// rather than emit that.
pub fn any_guard_remains(body: &[PreHirStmt], is_guard: impl Fn(&str) -> bool) -> bool {
    fn names(body: &[PreHirStmt], out: &mut Vec<String>) {
        for s in body {
            match s {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(n),
                    ..
                } => out.push(n.clone()),
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    names(then_body, out);
                    names(else_body, out);
                }
                PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                    names(body, out)
                }
                PreHirStmt::Block(inner) => names(inner, out),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    names(body, &mut found);
    found.iter().any(|n| is_guard(n))
}

/// A branch to bind: the node, its condition, and where each way goes.
#[derive(Debug, Clone)]
pub struct Branch {
    pub node: NodeId,
    pub cond: PreHirExpr,
    pub true_target: NodeId,
    pub false_target: NodeId,
}

/// Branch conditions bound to variables, and the edge guards that refer to
/// them.
#[derive(Debug, Clone, Default)]
pub struct BoundConditions {
    /// Statement to append to each branching node's body, in node order.
    pub bindings: Vec<(NodeId, PreHirStmt)>,
    /// Guard for each edge, over the bound variables.
    pub edges: HashMap<(NodeId, NodeId), PreHirExpr>,
}

impl BoundConditions {
    /// The closure [`crate::reaching_conditions::compute_reaching_conditions`]
    /// expects.
    pub fn edge_condition(&self) -> impl Fn(NodeId, NodeId) -> Option<PreHirExpr> + '_ {
        move |p, n| self.edges.get(&(p, n)).cloned()
    }
}

/// Bind each branch condition to a fresh boolean at the branch's own position.
///
/// See the module docs: this is what makes the reaching conditions safe to
/// re-evaluate anywhere, and therefore what makes DREAM emission sound in the
/// presence of memory the guards read.
pub fn materialize_branch_conditions(
    branches: &[Branch],
    mut fresh_name: impl FnMut(NodeId) -> String,
) -> BoundConditions {
    let mut out = BoundConditions::default();
    for branch in branches {
        // A self-deciding edge would make the variable refer to itself.
        if branch.true_target == branch.node || branch.false_target == branch.node {
            continue;
        }
        let name = fresh_name(branch.node);
        out.bindings.push((
            branch.node,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name.clone()),
                rhs: branch.cond.clone(),
            },
        ));
        let var = PreHirExpr::Var(name);
        out.edges
            .insert((branch.node, branch.true_target), var.clone());
        out.edges
            .insert((branch.node, branch.false_target), negate_expr(var));
    }
    out
}

/// Whether two guards are the arms of one decision, re-exported so a caller
/// can pair entries without reaching into the conditions module.
pub fn entries_are_exclusive(a: &GuardedEntry, b: &GuardedEntry) -> bool {
    conditions_are_complementary(&a.cond, &b.cond)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaching_conditions::compute_reaching_conditions;

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn stmt(name: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(1, fission_midend_core::ir::NirType::Bool),
        }
    }

    fn and(a: PreHirExpr, b: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs: Box::new(a),
            rhs: Box::new(b),
            ty: fission_midend_core::ir::NirType::Bool,
        }
    }

    fn assigned(stmts: &[PreHirStmt]) -> Vec<String> {
        stmts
            .iter()
            .filter_map(|s| match s {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(n),
                    ..
                } => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_shared_conjunct_is_factored_into_one_outer_if() {
        // A under a, B under a AND b, C under a AND !b.
        let entries = vec![
            GuardedEntry {
                cond: var("a"),
                stmts: vec![stmt("A")],
            },
            GuardedEntry {
                cond: and(var("a"), var("b")),
                stmts: vec![stmt("B")],
            },
            GuardedEntry {
                cond: and(var("a"), negate_expr(var("b"))),
                stmts: vec![stmt("C")],
            },
        ];
        let body = structure_guarded_entries(&entries);

        assert_eq!(body.len(), 1, "one outer if, got: {body:#?}");
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &body[0]
        else {
            panic!("expected an if, got {body:#?}");
        };
        assert_eq!(cond, &var("a"), "factored on the shared conjunct");
        assert!(else_body.is_empty(), "nothing runs when a is false");
        // Inside: A unconditionally, then the b decision.
        assert_eq!(assigned(then_body), vec!["A".to_string()]);
        let PreHirStmt::If {
            cond: inner,
            then_body: t,
            else_body: e,
        } = &then_body[1]
        else {
            panic!("expected a nested if, got {then_body:#?}");
        };
        assert_eq!(inner, &var("b"));
        assert_eq!(assigned(t), vec!["B".to_string()]);
        assert_eq!(assigned(e), vec!["C".to_string()], "!b became the else arm");
    }

    #[test]
    fn complementary_guards_become_one_if_else() {
        let entries = vec![
            GuardedEntry {
                cond: var("c"),
                stmts: vec![stmt("T")],
            },
            GuardedEntry {
                cond: negate_expr(var("c")),
                stmts: vec![stmt("F")],
            },
        ];
        assert!(entries_are_exclusive(&entries[0], &entries[1]));
        let body = structure_guarded_entries(&entries);
        assert_eq!(body.len(), 1);
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[0]
        else {
            panic!("expected an if");
        };
        assert_eq!(assigned(then_body), vec!["T".to_string()]);
        assert_eq!(assigned(else_body), vec!["F".to_string()]);
    }

    #[test]
    fn an_empty_then_arm_is_inverted_rather_than_shipped() {
        // Fission's HIR presentation invariants reject empty if shells, and
        // the match-fold driver produced one; this must not.
        let mut out = Vec::new();
        push_if(&mut out, var("c"), Vec::new(), vec![stmt("X")]);
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &out[0]
        else {
            panic!("expected an if");
        };
        assert_eq!(cond, &negate_expr(var("c")), "guard inverted");
        assert_eq!(assigned(then_body), vec!["X".to_string()]);
        assert!(else_body.is_empty());
    }

    #[test]
    fn unconditional_nodes_stay_flat() {
        let entries = vec![
            GuardedEntry {
                cond: always(),
                stmts: vec![stmt("A")],
            },
            GuardedEntry {
                cond: always(),
                stmts: vec![stmt("B")],
            },
        ];
        let body = structure_guarded_entries(&entries);
        assert_eq!(assigned(&body), vec!["A".to_string(), "B".to_string()]);
        assert!(
            !body.iter().any(|s| matches!(s, PreHirStmt::If { .. })),
            "no guard is needed when everything runs"
        );
    }

    #[test]
    fn a_diamond_round_trips_from_graph_to_if_else() {
        // 0 -> {1,2} -> 3, the classic diamond, all the way through.
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let branches = [Branch {
            node: 0,
            cond: var("raw"),
            true_target: 1,
            false_target: 2,
        }];
        let bound = materialize_branch_conditions(&branches, |n| format!("cond_{n}"));
        assert_eq!(bound.bindings.len(), 1, "one branch, one binding");

        let reaching =
            compute_reaching_conditions(&successors, 0, bound.edge_condition()).expect("acyclic");
        let order = vec![0, 1, 2, 3];
        let body = emit_acyclic_region(&order, &reaching, |n| vec![stmt(&format!("n{n}"))]);

        // n0, then one if/else over the bound variable, then n3.
        assert_eq!(assigned(&body), vec!["n0".to_string(), "n3".to_string()]);
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &body[1]
        else {
            panic!("expected an if between them, got {body:#?}");
        };
        assert_eq!(
            cond,
            &var("cond_0"),
            "the guard is the bound variable, not the raw expression"
        );
        assert_eq!(assigned(then_body), vec!["n1".to_string()]);
        assert_eq!(assigned(else_body), vec!["n2".to_string()]);
        assert!(
            !body.iter().any(|s| matches!(s, PreHirStmt::Goto(_))),
            "DREAM emission never produces a jump"
        );
    }

    #[test]
    fn side_effect_order_survives_across_independent_decisions() {
        // Two unrelated decisions in sequence must not be merged into one
        // run, or statements guarded by the second could move ahead of the
        // first.
        let entries = vec![
            GuardedEntry {
                cond: var("a"),
                stmts: vec![stmt("A")],
            },
            GuardedEntry {
                cond: var("b"),
                stmts: vec![stmt("B")],
            },
        ];
        let body = structure_guarded_entries(&entries);
        assert_eq!(body.len(), 2, "two separate ifs, got: {body:#?}");
        let PreHirStmt::If { cond: first, .. } = &body[0] else {
            panic!("expected an if");
        };
        let PreHirStmt::If { cond: second, .. } = &body[1] else {
            panic!("expected an if");
        };
        assert_eq!(first, &var("a"));
        assert_eq!(second, &var("b"));
    }
}
