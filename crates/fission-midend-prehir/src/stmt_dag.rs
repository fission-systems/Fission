//! Memo for walks over the `PreHir` statement graph.
//!
//! Structuring duplicates a tail by cloning the `Rc` that owns a body, not the
//! statements inside it, so what the printer renders as a tree is in memory a
//! DAG with heavy sharing. A walker that recurses without remembering where it
//! has been pays once per *path* instead of once per node.
//!
//! Measured on `openssh-portable` `ssh` `main`, inside one call:
//!
//! ```text
//! paths walked   61,040,505
//! real statements     6,409      -> 9,524x, and up to 19,664x on other calls
//! ```
//!
//! That multiplier, not the size of the function, is why `main` never
//! finished: 1,480 functions in that binary, and this one consumed the whole
//! hour the other 1,479 shared.
//!
//! Every walk memoised here answers a question about a statement alone, so
//! keying on the statement's address collapses the walk back onto the DAG and
//! leaves each answer unchanged. Addresses are unique among live statements
//! and stable for the length of one walk, which is all a memo lives for --
//! each public entry point allocates its own and drops it on return.

use crate::PreHirStmt;

/// Answers already computed in this walk, keyed by statement address.
pub type StmtMemo<V> = std::collections::HashMap<usize, V, rustc_hash::FxBuildHasher>;

/// The memo key for a statement: its address.
#[inline]
pub fn stmt_key(stmt: &PreHirStmt) -> usize {
    stmt as *const PreHirStmt as usize
}

/// Statements the body expands to when printed, counting a shared subtree
/// once per parent that names it -- which is once per time it is emitted.
///
/// Computed over the DAG, so it is cheap even when the expansion is not: the
/// number this returns can be astronomically larger than the memory the body
/// occupies, and telling those two apart is the whole point of having it.
/// Saturating, so a body past `usize` does not wrap into a small number.
pub fn expanded_stmt_count(body: &[PreHirStmt]) -> usize {
    let mut memo: StmtMemo<usize> = Default::default();
    body.iter().map(|s| expanded_of(s, &mut memo)).sum()
}

fn expanded_of(stmt: &PreHirStmt, memo: &mut StmtMemo<usize>) -> usize {
    let key = stmt_key(stmt);
    if let Some(cached) = memo.get(&key) {
        return *cached;
    }
    let mut total = 1usize;
    let add = |body: &[PreHirStmt], memo: &mut StmtMemo<usize>, total: &mut usize| {
        for inner in body {
            *total = total.saturating_add(expanded_of(inner, memo));
        }
    };
    match stmt {
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            add(then_body, memo, &mut total);
            add(else_body, memo, &mut total);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => add(body, memo, &mut total),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                add(&case.body, memo, &mut total);
            }
            add(default, memo, &mut total);
        }
        _ => {}
    }
    memo.insert(key, total);
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PreHirExpr;
    use std::rc::Rc;

    /// `levels` nested `if`s where both arms name the *same* body, which is how
    /// a collapse tier shares a tail between two branches. Printed, that is
    /// `2^(levels+1) - 1` statements; stored, it is `levels + 1`.
    fn both_arms_share_one_body(levels: usize) -> Vec<PreHirStmt> {
        let mut body = vec![PreHirStmt::Goto("L".to_string())];
        for _ in 0..levels {
            let shared = Rc::new(body);
            body = vec![PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: Rc::clone(&shared),
                else_body: shared,
            }];
        }
        body
    }

    #[test]
    fn a_shared_body_prints_once_per_parent_that_names_it() {
        // Twenty levels is 2,097,151 printed statements held in 21, and a
        // walk that does not memoise never returns from it. Both facts are the
        // point: the count must be over paths, and reaching it must not be.
        assert_eq!(
            expanded_stmt_count(&both_arms_share_one_body(20)),
            (1 << 21) - 1
        );
    }

    #[test]
    fn expansion_and_storage_agree_only_when_nothing_is_shared() {
        // No `Rc` is cloned here, so every statement has exactly one parent and
        // the expansion is just the statement count -- the case that made this
        // bug invisible for as long as it was.
        let body = vec![
            PreHirStmt::Goto("a".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: Rc::new(vec![PreHirStmt::Goto("b".to_string())]),
                else_body: Rc::new(vec![PreHirStmt::Goto("c".to_string())]),
            },
        ];
        assert_eq!(expanded_stmt_count(&body), 4);
    }

    #[test]
    fn a_body_past_usize_saturates_rather_than_wrapping() {
        // The ceiling that consumes this must not be talked under by an
        // expansion so large it wraps to a small number.
        assert_eq!(
            expanded_stmt_count(&both_arms_share_one_body(200)),
            usize::MAX
        );
    }
}
