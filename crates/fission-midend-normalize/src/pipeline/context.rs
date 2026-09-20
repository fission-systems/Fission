//! Per-function context owned by the normalize pipeline.
//!
//! Cleanup and constant-pointer passes need a small amount of information
//! that belongs to the current function rather than to the process: global
//! symbol facts and exception landing-pad labels. The pass implementations
//! still consume the historical thread-local slots internally, but their
//! installation and restoration live here so callers do not depend on those
//! slots directly.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    pub static GLOBAL_SYMBOL_CONTEXT: RefCell<Option<GlobalSymbolContext>> = RefCell::new(None);
    pub static PROTECTED_LSDA_LABELS: RefCell<HashSet<String>> =
        RefCell::new(HashSet::new());
}

/// Global symbol facts used by constant-pointer recovery.
#[derive(Clone, Default)]
pub struct GlobalSymbolContext {
    pub names: std::collections::HashMap<u64, String>,
    pub sizes: std::collections::HashMap<u64, u64>,
}

/// Per-function inputs needed by normalize passes.
#[derive(Clone, Default)]
pub struct NormalizeContext {
    pub global_symbols: Option<GlobalSymbolContext>,
    pub protected_lsda_labels: HashSet<String>,
}

impl NormalizeContext {
    pub fn new(
        global_symbols: GlobalSymbolContext,
        protected_lsda_labels: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            global_symbols: Some(global_symbols),
            protected_lsda_labels: protected_lsda_labels.into_iter().collect(),
        }
    }
}

/// Scoped installation of a [`NormalizeContext`] for the existing leaf-pass
/// APIs. Nested renders restore their caller's context, and unwinding restores
/// it as well.
pub struct NormalizeContextGuard {
    previous_global: Option<GlobalSymbolContext>,
    previous_protected: HashSet<String>,
    active: bool,
}

impl NormalizeContextGuard {
    pub fn install(context: &NormalizeContext) -> Self {
        let previous_global = GLOBAL_SYMBOL_CONTEXT.with(|slot| {
            let next = context.global_symbols.clone();
            std::mem::replace(&mut *slot.borrow_mut(), next)
        });
        let previous_protected = PROTECTED_LSDA_LABELS.with(|slot| {
            std::mem::replace(
                &mut *slot.borrow_mut(),
                context.protected_lsda_labels.clone(),
            )
        });
        Self {
            previous_global,
            previous_protected,
            active: true,
        }
    }

    pub fn clear(&mut self) {
        self.restore();
    }

    fn restore(&mut self) {
        if !self.active {
            return;
        }
        GLOBAL_SYMBOL_CONTEXT.with(|slot| {
            *slot.borrow_mut() = self.previous_global.take();
        });
        PROTECTED_LSDA_LABELS.with(|slot| {
            *slot.borrow_mut() = std::mem::take(&mut self.previous_protected);
        });
        self.active = false;
    }
}

impl Drop for NormalizeContextGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn restores_outer_context_when_normalization_unwinds() {
        let outer = NormalizeContext::new(
            GlobalSymbolContext {
                names: HashMap::from([(0x1000, "outer_global".to_string())]),
                sizes: HashMap::from([(0x1000, 4)]),
            },
            ["outer_landing_pad".to_string()],
        );
        let previous_global = GLOBAL_SYMBOL_CONTEXT
            .with(|slot| std::mem::replace(&mut *slot.borrow_mut(), outer.global_symbols.clone()));
        let previous_protected = PROTECTED_LSDA_LABELS.with(|slot| {
            std::mem::replace(&mut *slot.borrow_mut(), outer.protected_lsda_labels.clone())
        });

        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let inner = NormalizeContext::new(
                GlobalSymbolContext {
                    names: HashMap::from([(0x2000, "inner_global".to_string())]),
                    sizes: HashMap::from([(0x2000, 8)]),
                },
                ["inner_landing_pad".to_string()],
            );
            let _guard = NormalizeContextGuard::install(&inner);
            assert_eq!(
                GLOBAL_SYMBOL_CONTEXT.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .and_then(|ctx| ctx.names.get(&0x2000))
                        .cloned()
                }),
                Some("inner_global".to_string())
            );
            panic!("synthetic normalize failure");
        }));
        assert!(unwind.is_err());

        assert_eq!(
            GLOBAL_SYMBOL_CONTEXT.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .and_then(|ctx| ctx.names.get(&0x1000))
                    .cloned()
            }),
            Some("outer_global".to_string())
        );
        assert_eq!(
            PROTECTED_LSDA_LABELS.with(|slot| slot.borrow().clone()),
            outer.protected_lsda_labels
        );

        GLOBAL_SYMBOL_CONTEXT.with(|slot| {
            *slot.borrow_mut() = previous_global;
        });
        PROTECTED_LSDA_LABELS.with(|slot| {
            *slot.borrow_mut() = previous_protected;
        });
    }
}
