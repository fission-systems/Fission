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
        let Some(context_override) = self.low_bit_code_mode_override() else {
            return RuntimeAddressState::new(address, None);
        };
        RuntimeAddressState::new(address & !1, Some(context_override))
    }

    /// Builds the same context override [`normalize_low_bit_code_address`]
    /// derives from an odd (Thumb-bit-set) address, but unconditionally --
    /// for callers that already know they want low-bit code mode (e.g.
    /// Thumb) regardless of what the address's own low bit says.
    ///
    /// Some callers only have an *even* address for a Thumb-only target
    /// (ARMv7-M/Cortex-M has no ARM-mode execution at all, but toolchains and
    /// external tools frequently normalize away the ABI's bit-0 Thumb
    /// marker before reporting an address -- see e.g. DecBench's eval kit,
    /// which documents reporting "the even address" for ARM/Thumb targets).
    /// `normalize_low_bit_code_address` alone can't recover the mode in that
    /// case since it trusts the address's own bit 0; this lets a caller force
    /// it as a decode-failure fallback instead.
    pub fn low_bit_code_mode_override(&self) -> Option<PackedContextOverride> {
        let compiled = self.compiled.as_ref()?;

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
                return None;
            }
        }

        (context_override.mask_bits() != 0).then_some(context_override)
    }
}
