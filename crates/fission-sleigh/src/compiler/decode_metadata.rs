//! Post-`.sla` decode metadata normalization.
//!
//! Canonical SLEIGH execution must not derive cursor behavior from architecture
//! names or subtable-name policy tables. Any operand cursor behavior that is not
//! explicitly represented by decoded `.sla` operand/token metadata remains
//! unsupported at runtime rather than being repaired here.

use super::CompiledFrontend;

/// Normalize runtime decode metadata after `.sla` merge.
///
/// The old implementation populated x86 subtable-name cursor policy bits. That
/// made the compiled artifact depend on Fission-local knowledge of ModRM/SIB
/// tables instead of the Ghidra `.sla` owner chain. Keep the fields for artifact
/// compatibility, but clear them so canonical runtime cannot take that path.
pub fn apply_post_sla_decode_metadata(frontend: &mut CompiledFrontend) {
    for subtable in frontend.subtables.values_mut() {
        subtable.cursor_policy_bits = 0;
    }
    frontend.uses_shared_token_layout = false;
}
