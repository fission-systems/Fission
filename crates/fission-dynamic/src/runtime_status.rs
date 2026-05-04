//! Minimal runtime introspection — always available without OS debugger APIs.

use serde::Serialize;

/// Compile-time snapshot of which optional runtime stacks were enabled for this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DynamicRuntimeStatus {
    pub crate_name: &'static str,
    pub interactive_runtime_enabled: bool,
    pub unpacker_runtime_enabled: bool,
    pub platform: &'static str,
}

/// Returns feature flags compiled into this build and the host OS identifier (`std::env::consts::OS`).
#[must_use]
pub fn runtime_status() -> DynamicRuntimeStatus {
    DynamicRuntimeStatus {
        crate_name: "fission-dynamic",
        interactive_runtime_enabled: cfg!(feature = "interactive_runtime"),
        unpacker_runtime_enabled: cfg!(feature = "unpacker_runtime"),
        platform: std::env::consts::OS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_status_reports_default_features_disabled() {
        let s = runtime_status();
        assert_eq!(
            s.interactive_runtime_enabled,
            cfg!(feature = "interactive_runtime")
        );
        assert_eq!(
            s.unpacker_runtime_enabled,
            cfg!(feature = "unpacker_runtime")
        );
        #[cfg(not(feature = "interactive_runtime"))]
        assert!(!s.interactive_runtime_enabled);
        #[cfg(not(feature = "unpacker_runtime"))]
        assert!(!s.unpacker_runtime_enabled);
    }

    #[test]
    fn runtime_status_reports_platform() {
        let s = runtime_status();
        assert_eq!(s.platform, std::env::consts::OS);
    }

    #[test]
    fn runtime_status_crate_name_is_stable() {
        assert_eq!(runtime_status().crate_name, "fission-dynamic");
    }

    #[cfg(feature = "interactive_runtime")]
    #[test]
    fn runtime_status_reports_interactive_enabled_when_feature_on() {
        assert!(runtime_status().interactive_runtime_enabled);
    }

    #[cfg(feature = "unpacker_runtime")]
    #[test]
    fn runtime_status_reports_unpacker_enabled_when_feature_on() {
        assert!(runtime_status().unpacker_runtime_enabled);
    }
}
