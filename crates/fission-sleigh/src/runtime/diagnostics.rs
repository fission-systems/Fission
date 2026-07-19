// These flags are checked on the per-instruction decode hot path (every
// `walk_decision_tree`/`decode_subtable` call, every matched constructor's
// p-code template evaluation) — cached with `OnceLock` so the check is one
// syscall per process instead of one per instruction decoded.

pub(crate) fn terminal_reselect_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_TRACE_TERMINAL_RESELECT").is_some())
}

pub(crate) fn terminal_verify_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_TRACE_TERMINAL_VERIFY").is_some())
}

pub(crate) fn sleigh_build_debug_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_BUILD_DEBUG").is_some())
}
