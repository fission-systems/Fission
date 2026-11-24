use crate::prelude::*;

/// A type constraint derived from the context in which a variable is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UseConstraint {
    /// Variable is used as a memory address (Load/Store/Deref); must be a pointer.
    Ptr(NirType),
    /// Variable is used in a signed comparison; must be a signed integer.
    Signed { bits: u32 },
    /// Variable is used in an unsigned comparison; must be an unsigned integer.
    Unsigned { bits: u32 },
    /// Variable is the lhs of a logical right-shift (INT_RIGHT / SHR). Stronger than
    /// generic `Unsigned`: may demote a signed param so C `>>` stays logical.
    LogicalShiftUnsigned { bits: u32 },
    /// Variable is used in a context that requires exactly this type.
    Exact(NirType),
}

/// Merge a `UseConstraint` into a binding, returning `true` if the type changed.
///
/// The merge is monotone: types only move from weaker to stronger:
/// `Unknown < Int(unsigned) < Int(signed) < Ptr(Unknown) < Ptr(known)`.
/// An existing `Known` type is NEVER overwritten.
pub(super) fn merge_constraint(
    binding: &mut PreHirBinding,
    constraint: &UseConstraint,
    pointer_bits: u32,
) -> bool {
    if binding.surface_type_name.is_some() {
        return false;
    }
    match (&binding.ty, constraint) {
        // Already has a strong type — don't overwrite (except float upgrades).
        (NirType::Float { .. }, _) => false,
        (NirType::Bool, _) => false,
        // Full XMM vector bindings start as Aggregate(16) from PXOR zeroing;
        // FLOAT_ADD/MULT use evidence upgrades them to a float lane type.
        (NirType::Aggregate { .. }, UseConstraint::Exact(NirType::Float { bits })) => {
            binding.ty = NirType::Float { bits: *bits };
            true
        }
        (NirType::Aggregate { .. }, _) => false,

        // Same-width int pointee → float pointee when float ops prove element type.
        // Size-4 loads default to `uint*`; FLOAT_MULT use upgrades to `float*`.
        (NirType::Ptr(cur_pointee), UseConstraint::Ptr(NirType::Float { bits: float_bits })) => {
            match cur_pointee.as_ref() {
                NirType::Int { bits, .. } if bits == float_bits => {
                    binding.ty = NirType::Ptr(Box::new(NirType::Float { bits: *float_bits }));
                    true
                }
                NirType::Unknown => {
                    binding.ty = NirType::Ptr(Box::new(NirType::Float { bits: *float_bits }));
                    true
                }
                _ => false,
            }
        }
        (NirType::Ptr(_), _) => false,

        // Pointer constraint — only upgrade Unknown or pointer-width integer
        // (or non-stack integer bindings).
        (NirType::Unknown, UseConstraint::Ptr(pointee)) => {
            binding.ty = NirType::Ptr(Box::new(pointee.clone()));
            true
        }
        (NirType::Int { bits, .. }, UseConstraint::Ptr(pointee))
            if !matches!(binding.origin, Some(NirBindingOrigin::StackOffset(_)))
                || *bits == pointer_bits =>
        {
            binding.ty = NirType::Ptr(Box::new(pointee.clone()));
            true
        }
        (_, UseConstraint::Ptr(_)) => false,

        // Exact type from Cast context — only upgrade Unknown.
        (NirType::Unknown, UseConstraint::Exact(ty)) => {
            binding.ty = ty.clone();
            true
        }
        // Same-width int → float when float arithmetic uses the value.
        (NirType::Int { bits, .. }, UseConstraint::Exact(NirType::Float { bits: float_bits }))
            if bits == float_bits =>
        {
            binding.ty = NirType::Float { bits: *float_bits };
            true
        }
        (
            NirType::Int { .. },
            UseConstraint::Exact(NirType::Int {
                bits: new_bits,
                signed: new_signed,
            }),
        ) => {
            // Only change signedness if currently unsigned → promote to signed.
            if let NirType::Int {
                signed: cur_signed,
                bits: cur_bits,
            } = &binding.ty
            {
                if !*cur_signed && *new_signed && cur_bits == new_bits {
                    binding.ty = NirType::Int {
                        bits: *new_bits,
                        signed: true,
                    };
                    return true;
                }
            }
            false
        }
        (_, UseConstraint::Exact(_)) => false,

        // Signed/unsigned constraint — apply if Unknown or conflicting.
        (NirType::Unknown, UseConstraint::Signed { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: true,
            };
            true
        }
        (NirType::Unknown, UseConstraint::Unsigned { bits })
        | (NirType::Unknown, UseConstraint::LogicalShiftUnsigned { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: false,
            };
            true
        }
        (
            NirType::Int {
                signed: false,
                bits: cur_bits,
            },
            UseConstraint::Signed { bits: new_bits },
        ) if cur_bits == new_bits => {
            // Promote from unsigned to signed.
            binding.ty = NirType::Int {
                bits: *new_bits,
                signed: true,
            };
            true
        }
        (
            NirType::Int {
                signed: true,
                bits: cur_bits,
            },
            UseConstraint::LogicalShiftUnsigned { bits: new_bits },
        ) if cur_bits == new_bits => {
            // Demote signed scalars → unsigned only for logical SHR (INT_RIGHT).
            // Generic `Unsigned` must not undo signed promotion from signed
            // comparisons / casted arithmetic (see signed_casted_arithmetic test).
            binding.ty = NirType::Int {
                bits: *new_bits,
                signed: false,
            };
            true
        }
        _ => false,
    }
}

pub(super) fn has_exact_scalar_constraint(constraints: Option<&Vec<UseConstraint>>) -> bool {
    constraints.is_some_and(|constraints| {
        constraints.iter().any(|constraint| {
            matches!(
                constraint,
                UseConstraint::Exact(NirType::Int { .. } | NirType::Bool)
            )
        })
    })
}
