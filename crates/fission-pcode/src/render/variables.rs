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
use crate::midend::HashMap;

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
    /// Binary instruction addresses that define or use this local,
    /// when the builder can trace the rendered name to one scalar SSA value.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<u64>,
}

/// Byte width of a type, where it has one.
fn byte_size(ty: &NirType, pointer_size: u32) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } | NirType::Float { bits } if *bits > 0 && bits % 8 == 0 => {
            Some(bits / 8)
        }
        NirType::Ptr(_) => Some(pointer_size),
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
    pointer_size: u32,
) -> RecoveredVariable {
    RecoveredVariable {
        name: binding.name.clone(),
        type_name: binding
            .surface_type_name
            .clone()
            .unwrap_or_else(|| super::printer::print_type(&binding.ty)),
        stack_offset: frame_offset(binding.origin),
        size: byte_size(&binding.ty, pointer_size),
        kind,
        arg_index,
        addresses: Vec::new(),
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
    recovered_variables_with_instruction_addresses(func, &HashMap::default())
}

pub(crate) fn recovered_variables_with_instruction_addresses(
    func: &HirFunction,
    addresses_by_name: &HashMap<String, Vec<u64>>,
) -> Vec<RecoveredVariable> {
    let pointer_size = if func.is_64bit { 8 } else { 4 };
    let mut out = Vec::with_capacity(func.params.len() + func.locals.len());
    for (position, binding) in func.params.iter().enumerate() {
        let arg_index = match binding.origin {
            Some(NirBindingOrigin::ParamIndex(index)) => index,
            _ => position,
        };
        out.push(describe(binding, "arg", Some(arg_index), pointer_size));
    }
    for binding in &func.locals {
        let mut variable = describe(binding, "stack", None, pointer_size);
        if let Some(addresses) = addresses_by_name.get(&binding.name) {
            variable.addresses.clone_from(addresses);
        }
        out.push(variable);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_sizes_follow_function_target_for_arguments_and_locals() {
        for (is_64bit, expected_size) in [(false, 4), (true, 8)] {
            let mut function = HirFunction {
                is_64bit,
                ..HirFunction::default()
            };
            let binding = NirBinding {
                name: "pointer_argument".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            };
            function.params.push(binding.clone());
            function.locals.push(NirBinding {
                name: "pointer_local".to_string(),
                ty: NirType::Ptr(Box::new(binding.ty)),
                origin: Some(NirBindingOrigin::StackOffset(-16)),
                ..binding
            });

            let variables = recovered_variables(&function);
            assert_eq!(variables.len(), 2);
            assert!(
                variables
                    .iter()
                    .all(|variable| variable.size == Some(expected_size))
            );
            assert_eq!(variables[0].arg_index, Some(0));
            assert_eq!(variables[1].stack_offset, Some(-16));
        }
    }

    #[test]
    fn non_pointer_sizes_are_independent_of_target_pointer_width() {
        for pointer_size in [4, 8] {
            assert_eq!(byte_size(&NirType::Bool, pointer_size), Some(1));
            assert_eq!(
                byte_size(
                    &NirType::Int {
                        bits: 64,
                        signed: true
                    },
                    pointer_size
                ),
                Some(8)
            );
            assert_eq!(
                byte_size(&NirType::Float { bits: 32 }, pointer_size),
                Some(4)
            );
            assert_eq!(
                byte_size(
                    &NirType::Aggregate {
                        size: 12,
                        fields: vec![]
                    },
                    pointer_size
                ),
                Some(12)
            );
            assert_eq!(byte_size(&NirType::Unknown, pointer_size), None);
        }
    }

    #[test]
    fn serializes_instruction_addresses_for_traced_locals() {
        let mut function = HirFunction::default();
        function.locals.push(NirBinding {
            name: "local_value".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        let mut addresses = HashMap::default();
        addresses.insert("local_value".to_string(), vec![0x1000, 0x1004]);

        let variables = recovered_variables_with_instruction_addresses(&function, &addresses);
        assert_eq!(variables[0].addresses, vec![0x1000, 0x1004]);
        assert_eq!(
            serde_json::to_value(&variables[0]).unwrap()["addresses"],
            serde_json::json!([0x1000, 0x1004])
        );
    }

    #[test]
    fn omits_instruction_addresses_without_provenance() {
        let variable = RecoveredVariable {
            name: "local_value".to_string(),
            type_name: "int".to_string(),
            stack_offset: None,
            size: Some(4),
            kind: "stack",
            arg_index: None,
            addresses: Vec::new(),
        };

        assert!(
            serde_json::to_value(variable)
                .unwrap()
                .get("addresses")
                .is_none()
        );
    }
}
