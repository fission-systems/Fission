use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn prior_output_aliases_loop_carried_update(
        &self,
        prior: &Varnode,
        current: &Varnode,
    ) -> bool {
        !prior.is_constant
            && !current.is_constant
            && prior.space_id == current.space_id
            && is_register_space_id(prior.space_id)
            && prior.offset == current.offset
            && prior.size == 8
            && current.size == 4
            && self
                .register_namer()
                .hw_name_at(prior.offset, prior.size)
                .is_some()
    }

    pub(in crate::nir::builder) fn varnode_key_may_alias_output(
        candidate: &VarnodeKey,
        output_key: &VarnodeKey,
    ) -> bool {
        candidate == output_key
            || (is_register_space_id(candidate.space_id)
                && is_register_space_id(output_key.space_id)
                && candidate.space_id == output_key.space_id
                && Self::register_key_ranges_overlap(candidate, output_key))
    }

    pub(in crate::nir::builder) fn register_key_ranges_overlap(lhs: &VarnodeKey, rhs: &VarnodeKey) -> bool {
        let Some(lhs_end) = lhs.offset.checked_add(u64::from(lhs.size)) else {
            return false;
        };
        let Some(rhs_end) = rhs.offset.checked_add(u64::from(rhs.size)) else {
            return false;
        };
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }
}
