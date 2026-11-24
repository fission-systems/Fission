//! Regression tests for the cleanup passes.

mod assignments;
mod bindings;
mod call_targets;
mod casts_and_subvars;
mod conditions;
mod direct_returns;
mod labels;
mod loop_returns;
mod switch_guards;
mod temp_inlining;

use super::utils::*;
use super::*;
use crate::HashSet;
use fission_midend_core::*;
use fission_midend_prehir::*;

fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

fn preserved_temp_binding(name: &str, bits: u32) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn temp_binding(name: &str, bits: u32) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

fn stack_binding(name: &str, offset: i64) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(32),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(offset)),
        initializer: None,
    }
}
