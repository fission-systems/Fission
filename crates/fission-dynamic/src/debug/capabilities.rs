//! Machine-readable capability contract for debugger backends.
//!
//! Capability data describes what the selected backend implements in this
//! build. It does not promise that a process exists, is accessible, or will
//! execute successfully. Those runtime conditions are represented separately
//! from build/backend availability.

use serde::Serialize;

/// Current version of the serialized debugger-capabilities contract.
pub const DEBUG_CAPABILITIES_SCHEMA_VERSION: u16 = 1;

/// The selected debugger implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugBackendKind {
    /// Fission's in-process machine emulator.
    Emulator,
    /// Linux `ptrace` backend.
    LinuxPtrace,
    /// macOS native backend stub.
    #[serde(rename = "macos_native")]
    MacOsNative,
    /// Feature-gated Win32 native debugger.
    WindowsNative,
    /// A third-party backend that has not declared its identity.
    Unknown,
}

/// Whether the backend implementation is present in this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BackendAvailability {
    /// The backend implementation is present and can be selected.
    Available,
    /// The native/backend implementation is not usable in this build.
    Unavailable {
        /// Why the backend is unavailable.
        reason: CapabilityReason,
    },
}

/// A reason an operation or backend is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReason {
    /// The backend implementation does not provide this operation.
    OperationNotImplemented,
    /// Native debugging is not implemented on this platform.
    NativeDebuggerNotImplemented,
    /// The build-time feature required for this backend is disabled.
    BuildFeatureDisabled,
    /// The backend did not provide a capability report.
    CapabilityNotReported,
}

/// An Agent-facing debugger operation whose support can be queried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugOperation {
    /// Enumerate operating-system processes.
    ProcessEnumeration,
    /// Start a new target under the backend.
    Launch,
    /// Attach to an existing target.
    Attach,
    /// Detach from a target.
    Detach,
    /// Resume target execution.
    ContinueExecution,
    /// Poll for a structured target event.
    PollEvent,
    /// Execute one instruction.
    SingleStep,
    /// Step over the current call.
    StepOver,
    /// Run until the current function returns.
    StepOut,
    /// Pause a running target.
    Pause,
    /// Terminate the target.
    Terminate,
    /// Skip the current instruction.
    SkipInstruction,
    /// Set or remove software breakpoints.
    SoftwareBreakpoints,
    /// Set hardware breakpoints.
    HardwareBreakpoints,
    /// Set or remove data read/write watchpoints.
    MemoryWatchpoints,
    /// Set execution-address memory breakpoints.
    ExecuteMemoryBreakpoints,
    /// Set or remove DLL-load breakpoints.
    DllBreakpoints,
    /// Set or remove exception breakpoints.
    ExceptionBreakpoints,
    /// Enable or disable an existing breakpoint.
    BreakpointEnableDisable,
    /// List active breakpoints.
    BreakpointList,
    /// Read target registers.
    RegisterRead,
    /// Write target registers.
    RegisterWrite,
    /// Read individual condition flags.
    CpuFlagRead,
    /// Write individual condition flags.
    CpuFlagWrite,
    /// Read target memory.
    MemoryRead,
    /// Write target memory.
    MemoryWrite,
    /// Allocate memory in the target.
    RemoteMemoryAllocate,
    /// Free memory in the target.
    RemoteMemoryFree,
    /// Read target page protections.
    PageProtectionRead,
    /// Change target page protections.
    PageProtectionWrite,
    /// Read a value from the target stack.
    StackPeek,
    /// Pop a value from the target stack.
    StackPop,
    /// Push a value onto the target stack.
    StackPush,
    /// Search target memory for a byte pattern.
    MemoryPatternSearch,
    /// List modules observed by the backend.
    ModuleList,
    /// List target threads.
    ThreadList,
    /// Select a target thread.
    ThreadSwitch,
    /// Enumerate exports from a target module.
    ModuleExports,
    /// Enumerate imports from a target module.
    ModuleImports,
}

impl DebugOperation {
    /// Every operation in its stable serialized order.
    pub const ALL: [Self; 39] = [
        Self::ProcessEnumeration,
        Self::Launch,
        Self::Attach,
        Self::Detach,
        Self::ContinueExecution,
        Self::PollEvent,
        Self::SingleStep,
        Self::StepOver,
        Self::StepOut,
        Self::Pause,
        Self::Terminate,
        Self::SkipInstruction,
        Self::SoftwareBreakpoints,
        Self::HardwareBreakpoints,
        Self::MemoryWatchpoints,
        Self::ExecuteMemoryBreakpoints,
        Self::DllBreakpoints,
        Self::ExceptionBreakpoints,
        Self::BreakpointEnableDisable,
        Self::BreakpointList,
        Self::RegisterRead,
        Self::RegisterWrite,
        Self::CpuFlagRead,
        Self::CpuFlagWrite,
        Self::MemoryRead,
        Self::MemoryWrite,
        Self::RemoteMemoryAllocate,
        Self::RemoteMemoryFree,
        Self::PageProtectionRead,
        Self::PageProtectionWrite,
        Self::StackPeek,
        Self::StackPop,
        Self::StackPush,
        Self::MemoryPatternSearch,
        Self::ModuleList,
        Self::ThreadList,
        Self::ThreadSwitch,
        Self::ModuleExports,
        Self::ModuleImports,
    ];
}

/// A runtime condition that may still prevent an implemented operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRequirement {
    /// The host's ptrace policy and permissions allow the requested process.
    PtracePolicyAllowsAttach,
    /// The operating system permits debugging the selected process.
    TargetProcessAllowsDebugging,
    /// An emulated target has already been loaded into this backend.
    EmulatedMachineIsRunning,
    /// The target format and architecture are supported by the emulator.
    BinaryFormatAndArchitectureAreSupported,
}

/// Support for one operation, independent of backend build availability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OperationSupport {
    /// The backend implements this operation.
    Supported,
    /// The backend implements it, subject to the listed runtime conditions.
    Conditional {
        /// Conditions that the caller must check or be prepared to handle.
        requirements: Vec<RuntimeRequirement>,
    },
    /// The backend does not implement this operation in this build.
    Unsupported {
        /// A typed explanation, not just a backend error string.
        reason: CapabilityReason,
    },
}

/// Capability for one operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationCapability {
    /// Stable operation identifier.
    pub operation: DebugOperation,
    /// Whether this backend implements the operation.
    pub support: OperationSupport,
}

/// Host identity reported with the backend capabilities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DebugHostInfo {
    /// Rust target operating-system identifier, such as `linux` or `macos`.
    pub os: String,
    /// Rust target architecture identifier, such as `x86_64` or `aarch64`.
    pub architecture: String,
}

/// Complete, deterministic capability report for the selected backend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DebugBackendCapabilities {
    /// Version of this serialized report contract.
    pub schema_version: u16,
    /// Selected backend identity.
    pub backend: DebugBackendKind,
    /// Build host identity; target-specific runtime conditions are separate.
    pub host: DebugHostInfo,
    /// Whether the backend implementation is present in this build.
    pub availability: BackendAvailability,
    /// One entry for every [`DebugOperation`] in stable order.
    pub operations: Vec<OperationCapability>,
}

impl DebugBackendCapabilities {
    /// Build a report, marking omitted operations unsupported.
    pub fn new(
        backend: DebugBackendKind,
        availability: BackendAvailability,
        supported_operations: &[DebugOperation],
    ) -> Self {
        let unavailable_reason = match availability {
            BackendAvailability::Available => CapabilityReason::OperationNotImplemented,
            BackendAvailability::Unavailable { reason } => reason,
        };
        let operations = DebugOperation::ALL
            .into_iter()
            .map(|operation| OperationCapability {
                operation,
                support: if supported_operations.contains(&operation) {
                    OperationSupport::Supported
                } else {
                    OperationSupport::Unsupported {
                        reason: unavailable_reason,
                    }
                },
            })
            .collect();

        Self {
            schema_version: DEBUG_CAPABILITIES_SCHEMA_VERSION,
            backend,
            host: DebugHostInfo {
                os: std::env::consts::OS.to_string(),
                architecture: std::env::consts::ARCH.to_string(),
            },
            availability,
            operations,
        }
    }

    /// Mark an implemented operation as conditional on runtime requirements.
    pub fn conditionally_supporting(
        mut self,
        operation: DebugOperation,
        requirements: &[RuntimeRequirement],
    ) -> Self {
        if let Some(capability) = self
            .operations
            .iter_mut()
            .find(|capability| capability.operation == operation)
            && matches!(capability.support, OperationSupport::Supported)
        {
            capability.support = OperationSupport::Conditional {
                requirements: requirements.to_vec(),
            };
        }
        self
    }

    /// Find an operation's entry in this complete report.
    pub fn operation(&self, operation: DebugOperation) -> Option<&OperationCapability> {
        self.operations
            .iter()
            .find(|capability| capability.operation == operation)
    }

    /// Safe fallback for third-party backends that have not reported support.
    pub fn unreported() -> Self {
        Self::new(
            DebugBackendKind::Unknown,
            BackendAvailability::Unavailable {
                reason: CapabilityReason::CapabilityNotReported,
            },
            &[],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_every_operation_in_stable_order_and_typed_runtime_conditions() {
        let report = DebugBackendCapabilities::new(
            DebugBackendKind::LinuxPtrace,
            BackendAvailability::Available,
            &[DebugOperation::Attach, DebugOperation::MemoryRead],
        )
        .conditionally_supporting(
            DebugOperation::Attach,
            &[RuntimeRequirement::PtracePolicyAllowsAttach],
        );

        assert_eq!(report.operations.len(), DebugOperation::ALL.len());
        assert_eq!(
            report
                .operations
                .iter()
                .map(|entry| entry.operation)
                .collect::<Vec<_>>(),
            DebugOperation::ALL
        );
        assert_eq!(
            report.operation(DebugOperation::Attach).unwrap().support,
            OperationSupport::Conditional {
                requirements: vec![RuntimeRequirement::PtracePolicyAllowsAttach],
            }
        );
        assert_eq!(
            report.operation(DebugOperation::Launch).unwrap().support,
            OperationSupport::Unsupported {
                reason: CapabilityReason::OperationNotImplemented,
            }
        );

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["schema_version"], DEBUG_CAPABILITIES_SCHEMA_VERSION);
        assert_eq!(json["backend"], "linux_ptrace");
        assert_eq!(json["availability"]["status"], "available");
        let attach = json["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["operation"] == "attach")
            .unwrap();
        assert_eq!(attach["support"]["status"], "conditional");
        assert_eq!(
            attach["support"]["requirements"][0],
            "ptrace_policy_allows_attach"
        );
    }

    #[test]
    fn unreported_backends_fail_closed_instead_of_claiming_support() {
        let report = DebugBackendCapabilities::unreported();
        assert_eq!(report.backend, DebugBackendKind::Unknown);
        assert_eq!(
            report.availability,
            BackendAvailability::Unavailable {
                reason: CapabilityReason::CapabilityNotReported,
            }
        );
        assert!(report.operations.iter().all(|entry| matches!(
            entry.support,
            OperationSupport::Unsupported {
                reason: CapabilityReason::CapabilityNotReported
            }
        )));
    }
}
