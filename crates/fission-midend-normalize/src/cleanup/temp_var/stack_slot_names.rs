use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_prehir::util::{rename_vars_in_expr, rename_vars_in_stmts};

/// Reclaim a canonical `local_<stack-offset>` name after the binding that
/// originally occupied it has been removed by cleanup.
///
/// Builder name allocation must conservatively avoid every name it has seen,
/// so a real collision can produce `local_8_7`. At pipeline end the original
/// `local_8` may no longer exist, leaving a stale allocation suffix on the
/// surviving stack binding. Reclaiming the base here is a pure alpha rename:
/// it is admitted only for stack-derived bindings, only when the base is free,
/// and only when exactly one surviving binding claims that base.
pub fn canonicalize_orphaned_stack_slot_names(func: &mut PreHirFunction) -> bool {
    let occupied = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    let mut claimants: HashMap<String, Vec<String>> = HashMap::default();
    for binding in &func.locals {
        if !matches!(
            binding.origin,
            Some(NirBindingOrigin::StackOffset(_) | NirBindingOrigin::DerivedFromStackOffset(_))
        ) {
            continue;
        }
        let Some(base) = canonical_stack_slot_base(&binding.name) else {
            continue;
        };
        if occupied.contains(&base) {
            continue;
        }
        claimants
            .entry(base)
            .or_default()
            .push(binding.name.clone());
    }

    let mut renames = claimants
        .into_iter()
        .filter_map(|(base, names)| {
            (names.len() == 1).then(|| (names.into_iter().next().unwrap(), base))
        })
        .collect::<Vec<_>>();
    renames.sort();
    if renames.is_empty() {
        return false;
    }

    rename_vars_in_stmts(&mut func.body, &renames);
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if let Some(initializer) = &mut binding.initializer {
            rename_vars_in_expr(initializer, &renames);
        }
    }
    for binding in &mut func.locals {
        if let Some((_, replacement)) = renames.iter().find(|(name, _)| name == &binding.name) {
            binding.name = replacement.clone();
        }
    }
    true
}

fn canonical_stack_slot_base(name: &str) -> Option<String> {
    let suffix = name.strip_prefix("local_")?;
    let (offset, collision_id) = suffix.rsplit_once('_')?;
    if offset.is_empty()
        || collision_id.is_empty()
        || !offset.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !collision_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(format!("local_{offset}"))
}
