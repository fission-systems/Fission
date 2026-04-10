//! Memory slots, aggregate field recovery, and pointer-arithmetic recovery.

mod aggregate_fields;
mod partition;
mod ptr_arith;
mod slots;
mod typed_facts;

pub(crate) use aggregate_fields::apply_aggregate_fields_pass;
pub(crate) use partition::{partition_key_for_pointer_expr, PartitionKey};
pub(crate) use ptr_arith::apply_ptr_arith_recovery_pass;
pub(crate) use slots::{
    apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap, normalize_binding_initializers,
};
