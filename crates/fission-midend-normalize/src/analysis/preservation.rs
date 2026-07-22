//! Shared preserved-materialization policy helpers.
//!
//! These helpers keep the `TempPreserved` contract centralized so cleanup,
//! copy-propagation, and pipeline orchestration do not each carry slightly
//! different policy checks.

use fission_midend_core::NirBindingOrigin;
    use fission_midend_dir::DirBinding;
use crate::HashSet;

pub fn preserved_materialization_names(bindings: &[DirBinding]) -> HashSet<&str> {
    bindings
        .iter()
        .filter(|binding| binding.preserves_materialization())
        .map(|binding| binding.name.as_str())
        .collect()
}

pub fn should_block_trivial_return_collapse(
    name: &str,
    preserved_temps: &HashSet<&str>,
) -> bool {
    preserved_temps.contains(name)
}

pub fn should_skip_inline_for_preserved_temp(
    name: &str,
    preserved_temps: &HashSet<&str>,
) -> bool {
    preserved_temps.contains(name)
}

pub fn should_keep_unused_temp_binding(
    is_trivial_temp: bool,
    used: bool,
    initializer_has_side_effects: bool,
) -> bool {
    !is_trivial_temp || used || initializer_has_side_effects
}

pub fn should_skip_copyprop_for_preserved_name(
    name: &str,
    preserved_temps: &HashSet<&str>,
) -> bool {
    preserved_temps.contains(name)
}

pub fn preserved_binding_origin() -> NirBindingOrigin {
    NirBindingOrigin::TempPreserved
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use fission_midend_core::NirType;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn temp_binding(name: &str, origin: NirBindingOrigin) -> DirBinding {
        DirBinding {
            name: name.to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    #[test]
    fn preserved_materialization_names_collects_only_preserved_bindings() {
        let bindings = [
            temp_binding("uVar0", NirBindingOrigin::TempPreserved),
            temp_binding("uVar1", NirBindingOrigin::Temp),
        ];
        let names = preserved_materialization_names(&bindings);
        assert!(names.contains("uVar0"));
        assert!(!names.contains("uVar1"));
    }

    #[test]
    fn keep_unused_temp_binding_drops_dead_temp_preserved_without_side_effects() {
        assert!(!should_keep_unused_temp_binding(true, false, false));
        assert!(should_keep_unused_temp_binding(true, true, false));
        assert!(should_keep_unused_temp_binding(false, false, false));
    }

    #[test]
    fn skip_copyprop_for_any_preserved_name() {
        let preserved = ["uVar0"].into_iter().collect::<HashSet<_>>();
        assert!(should_skip_copyprop_for_preserved_name("uVar0", &preserved));
        assert!(!should_skip_copyprop_for_preserved_name(
            "uVar1", &preserved
        ));
    }
}
