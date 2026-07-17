//! Memory slots, aggregate field recovery, and pointer-arithmetic recovery.

mod aggregate_fields;
mod constant_ptr;
mod heritage;
mod partition;
mod ptr_arith;
mod slots;
mod split_datatype;
mod typed_facts;
mod union_resolve;

pub(crate) use aggregate_fields::{
    apply_aggregate_alias_access_rewrite_pass, apply_aggregate_fields_pass,
};
pub(crate) use constant_ptr::apply_constant_ptr_recovery_pass;
pub(crate) use heritage::apply_memory_heritage;
pub(crate) use partition::{PartitionKey, partition_key_for_pointer_expr};
pub(crate) use ptr_arith::{apply_ptr_arith_recovery_pass, apply_zero_index_deref_pass};
pub(crate) use slots::{
    apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap, normalize_binding_initializers,
};
pub(crate) use split_datatype::apply_split_datatype_pass;
pub(crate) use union_resolve::apply_union_resolve_pass;
