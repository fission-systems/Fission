//! PE-specific loader facts attached only to the identity report (`pe` field).

use crate::loader::LoadedBinary;

use super::model::PeIdentitySummary;

#[must_use]
pub(super) fn summarize_pe_identity(binary: &LoadedBinary) -> Option<PeIdentitySummary> {
    let facts = crate::loader::pe::identity_pe_facts(binary)?;
    let entry_section = binary
        .sections
        .iter()
        .find(|s| {
            binary.entry_point >= s.virtual_address
                && binary.entry_point < s.virtual_address.saturating_add(s.virtual_size)
        })
        .map(|s| s.name.clone());

    Some(PeIdentitySummary {
        tls_directory_present: Some(facts.tls_directory_present),
        tls_callback_count: Some(facts.tls_callback_count),
        debug_directory_kinds: facts.debug_directory_kinds,
        entry_section,
    })
}
