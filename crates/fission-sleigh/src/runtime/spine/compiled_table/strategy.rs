use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeDecodeStrategyKind {
    NativeFirst,
    CommonOnly,
    NativeDisabledReason(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeDecodeStrategy<'a> {
    native: Option<&'a Arc<NativeBackend>>,
    kind: RuntimeDecodeStrategyKind,
}

impl<'a> RuntimeDecodeStrategy<'a> {
    pub(super) fn for_table(
        compiled: &CompiledFrontend,
        native: Option<&'a Arc<NativeBackend>>,
        table_name: &str,
        ctx: &CompiledInstructionContext<'_>,
    ) -> Self {
        let Some(native) = native else {
            return Self {
                native: None,
                kind: RuntimeDecodeStrategyKind::CommonOnly,
            };
        };
        if native_backend_allowed(compiled, table_name, ctx) {
            Self {
                native: Some(native),
                kind: RuntimeDecodeStrategyKind::NativeFirst,
            }
        } else {
            Self {
                native: None,
                kind: RuntimeDecodeStrategyKind::NativeDisabledReason(
                    "context_dependent_decision_tree",
                ),
            }
        }
    }

    pub(super) fn native_for_table(
        &self,
        compiled: &CompiledFrontend,
        table_name: &str,
        ctx: &CompiledInstructionContext<'_>,
    ) -> Option<&'a Arc<NativeBackend>> {
        let native = self.native?;
        native_backend_allowed(compiled, table_name, ctx).then_some(native)
    }

    #[allow(dead_code)]
    pub(super) fn kind(&self) -> RuntimeDecodeStrategyKind {
        self.kind
    }
}

fn native_backend_allowed(
    compiled: &CompiledFrontend,
    table_name: &str,
    ctx: &CompiledInstructionContext<'_>,
) -> bool {
    let Some(subtable) = compiled.subtables.get(table_name) else {
        return false;
    };
    if subtable.sla_subtable_id != 0
        || subtable
            .decision_tree
            .nodes
            .iter()
            .any(|node| !node.leaf_entries.is_empty())
    {
        // Native backends currently return a constructor slot only.  Ghidra
        // .sla identity requires terminal DisjointPattern verification and
        // subtable/constructor-id lookup before a constructor is final.  Until
        // codegen emits the same checked terminal verifier, native remains an
        // acceleration target for legacy tables only.
        // After build_frontend_from_sla_native_model, all SLA-loaded subtables
        // have sla_subtable_id != 0 or leaf_entries, so this returns false for
        // all current architectures.
        return false;
    }
    // Note: the previous `shared_token_cursor` short-circuit that allowed native
    // for x86 has been removed. For SLA-migrated frontends the check above already
    // returns false for all subtables that have SLA identity. The shared_token_cursor
    // heuristic was x86-specific and is no longer a valid gate for architecture-neutral
    // native backend selection.
    !subtable
        .decision_tree
        .nodes
        .iter()
        .any(|node| match node.probe {
            CompiledDecisionProbe::ContextBitSlice { offset, mask, .. } => {
                let relevant_mask = u64::from(mask) << offset;
                (ctx.context_known_mask & relevant_mask) != relevant_mask
            }
            CompiledDecisionProbe::SlaContextBits { .. } => true,
            _ => false,
        })
}
