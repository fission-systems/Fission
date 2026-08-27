//! Semantic variable naming: replace generic `param_N`/`uVarN`/`iVarN`/
//! `xVarN`/`bVarN` placeholder names with readable ones inferred from usage
//! patterns (pointer dereference / linked-list traversal, well-known libc
//! argument roles, loop induction variables).
//!
//! Runs once, after `apply_hir_presentation_passes`' structural fixed-point
//! loop settles: loop-counter detection needs the final `While`/`For`/
//! `DoWhile` shape, and running once avoids each earlier iteration's
//! transient tree shapes producing different (and possibly conflicting)
//! guesses. Pure presentation sugar under ADR 0011 -- a rename changes no
//! evaluation, order, or observable side effect, only the printed label.
//!
//! Architecture mirrors angr's `analyses/decompiler/semantic_naming`: a
//! priority-ordered chain of independent pattern detectors, each producing
//! score-gated candidate renames; once a variable is claimed by a
//! higher-priority pattern, later patterns skip it.

mod loop_counter_naming;
mod pointer_naming;
mod size_naming;
#[cfg(test)]
mod tests;
mod util;

use super::{HirExpr, HirFunction, HirLValue, HirStmt};
use std::collections::HashSet;

use util::{is_renamable_temp_name, rename_var_everywhere};

/// One candidate rename a detector proposes, with a confidence score used
/// both as its own admission threshold and to break ties when multiple
/// detectors in the same pass want the same variable.
pub(super) struct Candidate {
    pub name: String,
    pub new_name: String,
    pub score: u32,
}

/// Apply every registered naming pass in priority order. Returns `true` if
/// any variable was renamed.
pub(crate) fn apply_semantic_naming(func: &mut HirFunction) -> bool {
    let mut already_named: HashSet<String> = HashSet::new();
    let mut changed = false;

    // (priority, candidates) so passes stay sorted without needing dynamic
    // dispatch on a trait object per pass.
    let mut all: Vec<(u32, Vec<Candidate>)> = vec![
        (
            loop_counter_naming::PRIORITY,
            loop_counter_naming::candidates(func),
        ),
        (pointer_naming::PRIORITY, pointer_naming::candidates(func)),
        (size_naming::PRIORITY, size_naming::candidates(func)),
    ];
    all.sort_by_key(|(priority, _)| *priority);

    for (_, candidates) in all {
        // Within one pass, higher-scored candidates claim a generated
        // display name first (e.g. the first-detected loop counter gets
        // `i`, the second `j`).
        let mut sorted = candidates;
        sorted.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        for candidate in sorted {
            if already_named.contains(&candidate.name) {
                continue;
            }
            // Re-check renamability against the *current* tree: an earlier
            // pass in this same call may have already consumed the name
            // (e.g. two candidates both computed against the pre-rename
            // tree happened to target the same original binding).
            if !binding_still_generic(func, &candidate.name) {
                continue;
            }
            if already_named.contains(&candidate.new_name) {
                continue;
            }
            rename_var_everywhere(func, &candidate.name, &candidate.new_name);
            already_named.insert(candidate.name);
            already_named.insert(candidate.new_name);
            changed = true;
        }
    }

    changed
}

fn binding_still_generic(func: &HirFunction, name: &str) -> bool {
    func.params
        .iter()
        .chain(func.locals.iter())
        .any(|b| b.name == name && is_renamable_temp_name(&b.name))
}
