//! The variables a decompilation recovered, as data rather than as C text.
//!
//! A consumer that wants to know what fission recovered -- a type-recovery
//! evaluator, a GUI variable pane -- otherwise has to parse the printed
//! declarations back out of the C. That parse loses exactly what it most
//! needs: a stack slot's offset survives only when the name happens to spell
//! it, an argument's ABI position is not written down anywhere, and a
//! multi-word type name has to be told apart from the identifier following
//! it. All of it is already in the `HirFunction` this module reads.

use super::{HirFunction, NirBinding, NirBindingOrigin, NirType};

/// One variable a decompilation recovered.
///
/// Field names match the shape consumers expect on the wire, so the JSON can
/// be handed straight across without a translation table on the far side.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RecoveredVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// Frame offset for a stack slot; `None` for an argument or a
    /// register-resident local, which have no slot to name.
    pub stack_offset: Option<i64>,
    pub size: Option<u32>,
    /// `"arg"` or `"stack"`.
    pub kind: &'static str,
    /// Position in the ABI argument order, for an argument.
    pub arg_index: Option<usize>,
}

/// Byte width of a type, where it has one.
fn byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } | NirType::Float { bits } if *bits > 0 && bits % 8 == 0 => {
            Some(bits / 8)
        }
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        _ => None,
    }
}

/// The frame offset a binding's origin names, where it names one.
///
/// `OutgoingArgSlot` is deliberately absent: it addresses the *callee's*
/// incoming argument area, not a slot holding a variable of this function.
fn frame_offset(origin: Option<NirBindingOrigin>) -> Option<i64> {
    match origin? {
        NirBindingOrigin::StackOffset(offset)
        | NirBindingOrigin::HomeSlot(offset)
        | NirBindingOrigin::DerivedFromStackOffset(offset) => Some(offset),
        _ => None,
    }
}

fn describe(
    binding: &NirBinding,
    kind: &'static str,
    arg_index: Option<usize>,
) -> RecoveredVariable {
    RecoveredVariable {
        name: binding.name.clone(),
        type_name: binding
            .surface_type_name
            .clone()
            .unwrap_or_else(|| super::printer::print_type(&binding.ty)),
        stack_offset: frame_offset(binding.origin),
        size: byte_size(&binding.ty),
        kind,
        arg_index,
    }
}

/// The recovered variables of `func`, arguments first in ABI order.
///
/// Every local is reported, `Temp` origin included. That origin records how a
/// binding was *introduced* -- lowering needed somewhere to put a value --
/// and says nothing about whether the program had a variable there. By this
/// point the debug-info overlay has run, so a binding introduced as a temp
/// can be carrying a real name and a real type; `fill_window`'s `n` and `m`
/// are gzip's own variables and both arrive here as temps. Filtering on the
/// flag dropped them, and with them the only thing a consumer could have
/// matched.
pub fn recovered_variables(func: &HirFunction) -> Vec<RecoveredVariable> {
    let mut out = Vec::with_capacity(func.params.len() + func.locals.len());
    for (position, binding) in func.params.iter().enumerate() {
        let arg_index = match binding.origin {
            Some(NirBindingOrigin::ParamIndex(index)) => index,
            _ => position,
        };
        out.push(describe(binding, "arg", Some(arg_index)));
    }
    for binding in &func.locals {
        out.push(describe(binding, "stack", None));
    }
    out
}
