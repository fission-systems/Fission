#![cfg(feature = "interactive_runtime")]

use fission_dynamic::debug::{
    BackendAvailability, CapabilityReason, DebugBackendCapabilities, DebugBackendKind,
    DebugOperation, ExecutionBackend, OperationSupport, RuntimeRequirement,
};

fn support(report: &DebugBackendCapabilities, operation: DebugOperation) -> &OperationSupport {
    &report
        .operation(operation)
        .expect("every operation has an explicit capability")
        .support
}

#[test]
fn emulator_capabilities_match_its_agent_operations() {
    let backend = fission_dynamic::debug::EmulatorBackend::new();
    let report = backend.capabilities();

    assert_eq!(report.backend, DebugBackendKind::Emulator);
    assert_eq!(report.availability, BackendAvailability::Available);
    assert_eq!(report.operations.len(), DebugOperation::ALL.len());
    assert_eq!(
        support(&report, DebugOperation::Launch),
        &OperationSupport::Conditional {
            requirements: vec![RuntimeRequirement::BinaryFormatAndArchitectureAreSupported],
        }
    );
    assert_eq!(
        support(&report, DebugOperation::Attach),
        &OperationSupport::Conditional {
            requirements: vec![RuntimeRequirement::EmulatedMachineIsRunning],
        }
    );
    assert_eq!(
        support(&report, DebugOperation::MemoryWatchpoints),
        &OperationSupport::Supported
    );
    assert_eq!(
        support(&report, DebugOperation::ExecuteMemoryBreakpoints),
        &OperationSupport::Unsupported {
            reason: CapabilityReason::OperationNotImplemented,
        }
    );
    assert_eq!(
        support(&report, DebugOperation::ProcessEnumeration),
        &OperationSupport::Unsupported {
            reason: CapabilityReason::OperationNotImplemented,
        }
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_capabilities_separate_ptrace_permission_from_build_support() {
    let backend = fission_dynamic::debug::platform::linux::LinuxDebugger::new();
    let report = backend.capabilities();

    assert_eq!(report.backend, DebugBackendKind::LinuxPtrace);
    assert_eq!(report.availability, BackendAvailability::Available);
    assert_eq!(
        support(&report, DebugOperation::Launch),
        &OperationSupport::Conditional {
            requirements: vec![RuntimeRequirement::PtracePolicyAllowsLaunch],
        }
    );
    assert_eq!(
        support(&report, DebugOperation::Attach),
        &OperationSupport::Conditional {
            requirements: vec![RuntimeRequirement::PtracePolicyAllowsAttach],
        }
    );
    assert_eq!(
        support(&report, DebugOperation::SingleStep),
        &OperationSupport::Supported
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_declares_that_process_control_needs_the_mach_task_port() {
    let backend = fission_dynamic::debug::platform::macos::MacOSDebugger::new();
    let report = backend.capabilities();

    assert_eq!(report.backend, DebugBackendKind::MacOsNative);
    assert_eq!(report.availability, BackendAvailability::Available);
    assert_eq!(
        support(&report, DebugOperation::ProcessEnumeration),
        &OperationSupport::Supported
    );
    // Launch and attach work only for a caller macOS has authorized to take a
    // task port; the report says so instead of claiming unconditional support.
    for operation in [DebugOperation::Launch, DebugOperation::Attach] {
        match support(&report, operation) {
            OperationSupport::Conditional { requirements } => assert!(
                requirements.contains(&RuntimeRequirement::MachTaskPortAccess),
                "{operation:?}: {requirements:?}"
            ),
            other => panic!("{operation:?} should be conditional, not {other:?}"),
        }
    }
    for operation in [
        DebugOperation::SingleStep,
        DebugOperation::ContinueExecution,
        DebugOperation::SoftwareBreakpoints,
        DebugOperation::RegisterRead,
        DebugOperation::MemoryRead,
        DebugOperation::MemoryWrite,
    ] {
        assert_eq!(
            support(&report, operation),
            &OperationSupport::Supported,
            "{operation:?}"
        );
    }
}

#[cfg(all(target_os = "windows", not(feature = "windows_native_debugger")))]
#[test]
fn windows_reports_the_native_feature_gate() {
    let backend = fission_dynamic::debug::platform::windows::WindowsDebugger::new();
    let report = backend.capabilities();

    assert_eq!(report.backend, DebugBackendKind::WindowsNative);
    assert_eq!(
        report.availability,
        BackendAvailability::Unavailable {
            reason: CapabilityReason::BuildFeatureDisabled,
        }
    );
    assert_eq!(
        support(&report, DebugOperation::Attach),
        &OperationSupport::Unsupported {
            reason: CapabilityReason::BuildFeatureDisabled,
        }
    );
}

#[cfg(all(target_os = "windows", feature = "windows_native_debugger"))]
#[test]
fn windows_reports_native_debugging_when_the_feature_is_enabled() {
    let backend = fission_dynamic::debug::platform::windows::WindowsDebugger::new();
    let report = backend.capabilities();

    assert_eq!(report.backend, DebugBackendKind::WindowsNative);
    assert_eq!(report.availability, BackendAvailability::Available);
    assert_eq!(
        support(&report, DebugOperation::Attach),
        &OperationSupport::Conditional {
            requirements: vec![RuntimeRequirement::TargetProcessAllowsDebugging],
        }
    );
    assert_eq!(
        support(&report, DebugOperation::MemoryRead),
        &OperationSupport::Supported
    );
}
