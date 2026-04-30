pub(crate) fn terminal_reselect_trace_enabled() -> bool {
    std::env::var_os("FISSION_TRACE_TERMINAL_RESELECT").is_some()
}
