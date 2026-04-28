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
    if CompiledTokenCursorPolicy::for_frontend(compiled).uses_legacy_shared_tokens() {
        return true;
    }
    let Some(subtable) = compiled.subtables.get(table_name) else {
        return false;
    };
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
