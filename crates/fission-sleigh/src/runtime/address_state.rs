use super::*;
use crate::packed_context::PackedContextOverride;

const LOW_BIT_CODE_CONTEXT_FIELDS: [&str; 4] = ["TMode", "T", "ISA_MODE", "LowBitCodeMode"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAddressState {
    pub address: u64,
    pub context_override: Option<PackedContextOverride>,
}

impl RuntimeAddressState {
    pub const fn new(address: u64, context_override: Option<PackedContextOverride>) -> Self {
        Self {
            address,
            context_override,
        }
    }
}

impl RuntimeSleighFrontend {
    pub fn normalize_low_bit_code_address(&self, address: u64) -> RuntimeAddressState {
        if address & 1 == 0 {
            return RuntimeAddressState::new(address, None);
        }
        let Some(compiled) = self.compiled.as_ref() else {
            return RuntimeAddressState::new(address, None);
        };

        let mut context_override = PackedContextOverride::default();
        for name in LOW_BIT_CODE_CONTEXT_FIELDS {
            let Some(field) = compiled
                .language_layout
                .context_fields
                .iter()
                .find(|field| field.name == name)
            else {
                continue;
            };
            if context_override
                .set_bits(field.bit_offset, field.bit_width, 1)
                .is_err()
            {
                return RuntimeAddressState::new(address, None);
            }
        }

        if context_override.mask_bits() == 0 {
            RuntimeAddressState::new(address, None)
        } else {
            RuntimeAddressState::new(address & !1, Some(context_override))
        }
    }
}
