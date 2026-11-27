#![cfg(feature = "interactive_runtime")]
//! Integration tests for issue #1264:
//! - Process capture identity stability & PID reuse refusal
//! - Explicit consistency epochs (StoppedAllObservedThreads vs LiveBestEffort)
//! - Thread/mapping churn & partial enumeration diagnostics
//! - Short reads & unmapped gaps remaining bounded and explicit
//! - Raw debugger-patched bytes preserved separately from opt-in logical unpatched view

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use fission_core::Result as FissionResult;
use fission_dynamic::debug::capabilities::{
    BackendAvailability, DebugBackendCapabilities, DebugBackendKind, DebugOperation,
};
use fission_dynamic::debug::memory::SparseReadOptions;
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::{
    Breakpoint, BreakpointKind, CaptureCoherenceStatus, CaptureConsistency, DebugState,
    DebugStatus, DebuggerInterventionKind, EnumerationCompleteness, LiveAcquisitionOptions,
    MemoryViewKind, ModuleAddressTransform, ModuleListCompleteness, ModuleListReport,
    ProcessCaptureIdentity, ProcessInfo, ProcessMemoryMapping, ProcessModule, RegisterState,
    ThreadInfo,
};

struct MockInner {
    state: DebugState,
    lifetime_token: Option<String>,
    memory: BTreeMap<u64, u8>,
    modules: Vec<ProcessModule>,
    module_completeness: ModuleListCompleteness,
    breakpoints: Vec<Breakpoint>,
    pause_called: usize,
    read_call_hook: Option<Box<dyn FnMut(usize, &mut MockInner) + Send>>,
    read_calls: usize,
}

struct MockBackend {
    capabilities: DebugBackendCapabilities,
    inner: Arc<Mutex<MockInner>>,
}

impl MockBackend {
    fn full_capabilities() -> DebugBackendCapabilities {
        DebugBackendCapabilities::new(
            DebugBackendKind::LinuxPtrace,
            BackendAvailability::Available,
            &[DebugOperation::MemoryRead, DebugOperation::ModuleList],
        )
    }

    fn new_stopped(pid: u32, token: &str) -> Self {
        let mut threads = BTreeMap::new();
        threads.insert(
            1,
            ThreadInfo {
                thread_id: 1,
                start_address: 0x401000,
                suspended: true,
                is_main: true,
            },
        );
        let state = DebugState {
            attached_pid: Some(pid),
            main_thread_id: Some(1),
            current_thread_id: Some(1),
            status: DebugStatus::Suspended,
            threads,
            ..Default::default()
        };
        let modules = vec![ProcessModule {
            path: "/bin/target".to_string(),
            name: "target".to_string(),
            device: Some("08:01".to_string()),
            inode: Some(42),
            mappings: vec![ProcessMemoryMapping {
                runtime_start: 0x400000,
                runtime_end: 0x402000,
                file_offset: Some(0),
                permissions: Some("r-xp".to_string()),
                path: Some("/bin/target".to_string()),
            }],
            address_transform: ModuleAddressTransform::Resolved {
                format: "ELF".to_string(),
                analysis_address_domain: "elf-va".to_string(),
                runtime_address_domain: "runtime".to_string(),
                formula: "runtime = va + 0x0".to_string(),
                load_bias: 0,
            },
        }];

        Self {
            capabilities: Self::full_capabilities(),
            inner: Arc::new(Mutex::new(MockInner {
                state,
                lifetime_token: Some(token.to_string()),
                memory: BTreeMap::new(),
                modules,
                module_completeness: ModuleListCompleteness::Complete,
                breakpoints: Vec::new(),
                pause_called: 0,
                read_call_hook: None,
                read_calls: 0,
            })),
        }
    }

    fn write_bytes(&self, start: u64, bytes: &[u8]) {
        let mut g = self.inner.lock().unwrap();
        for (i, &b) in bytes.iter().enumerate() {
            g.memory.insert(start + i as u64, b);
        }
    }
}

impl ExecutionBackend for MockBackend {
    fn capabilities(&self) -> DebugBackendCapabilities {
        self.capabilities.clone()
    }

    fn enumerate_processes() -> Vec<ProcessInfo> {
        Vec::new()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        self.inner.lock().unwrap().state.attached_pid = Some(pid);
        Ok(())
    }

    fn detach(&mut self) -> FissionResult<()> {
        self.inner.lock().unwrap().state.attached_pid = None;
        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.inner.lock().unwrap().state.attached_pid.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        self.inner.lock().unwrap().state.attached_pid
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        Ok(())
    }

    fn single_step(&mut self) -> FissionResult<()> {
        Ok(())
    }

    fn pause(&mut self) -> FissionResult<()> {
        self.inner.lock().unwrap().pause_called += 1;
        Ok(())
    }

    fn set_sw_breakpoint(&mut self, _address: u64) -> FissionResult<()> {
        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, _address: u64) -> FissionResult<()> {
        Ok(())
    }

    fn list_breakpoints(&self) -> Vec<Breakpoint> {
        self.inner.lock().unwrap().breakpoints.clone()
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let mut g = self.inner.lock().unwrap();
        g.read_calls += 1;
        let call_idx = g.read_calls;
        if let Some(mut hook) = g.read_call_hook.take() {
            hook(call_idx, &mut g);
            g.read_call_hook = Some(hook);
        }
        let mut out = Vec::with_capacity(size);
        for off in 0..size {
            let addr = address + off as u64;
            match g.memory.get(&addr) {
                Some(&b) => out.push(b),
                None => {
                    return Err(fission_core::err!(debug, "unmapped memory at 0x{addr:x}"));
                }
            }
        }
        Ok(out)
    }

    fn read_memory_partial(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let g = self.inner.lock().unwrap();
        let mut out = Vec::new();
        for off in 0..size {
            match g.memory.get(&(address + off as u64)) {
                Some(&b) => out.push(b),
                None => break,
            }
        }
        if out.is_empty() {
            Err(fission_core::err!(
                debug,
                "unmapped memory at 0x{address:x}"
            ))
        } else {
            Ok(out)
        }
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        self.write_bytes(address, data);
        Ok(())
    }

    fn fetch_registers(&mut self, _thread_id: u32) -> FissionResult<RegisterState> {
        Ok(RegisterState::default())
    }

    fn get_state(&self) -> DebugState {
        self.inner.lock().unwrap().state.clone()
    }

    fn list_modules(&mut self) -> FissionResult<ModuleListReport> {
        let g = self.inner.lock().unwrap();
        Ok(ModuleListReport {
            schema_version: 1,
            pid: g.state.attached_pid.unwrap_or(0),
            completeness: g.module_completeness,
            modules: g.modules.clone(),
            other_mappings: Vec::new(),
            diagnostics: Vec::new(),
        })
    }

    fn capture_identity(&self, session_id: &str) -> Option<ProcessCaptureIdentity> {
        let g = self.inner.lock().unwrap();
        let pid = g.state.attached_pid?;
        Some(ProcessCaptureIdentity {
            host_id: Some("mock-host".to_string()),
            session_id: session_id.to_string(),
            pid,
            lifetime_token: g.lifetime_token.clone(),
            architecture: Some("x86_64".to_string()),
        })
    }
}

#[test]
fn pid_reuse_mid_acquisition_is_refused_and_not_merged() {
    let mut backend = MockBackend::new_stopped(4242, "start:100");
    backend.write_bytes(0x400000, b"\x7fELF\x02\x01\x01\x00");
    {
        let mut g = backend.inner.lock().unwrap();
        // Simulate OS reusing the exact same numeric PID with a new start time during read.
        g.read_call_hook = Some(Box::new(|call, st| {
            if call == 1 {
                st.lifetime_token = Some("start:999".to_string());
            }
        }));
    }

    let cap = backend
        .acquire_epoch_bound_memory(0x400000, 8, LiveAcquisitionOptions::default())
        .expect("refusal returns explicit InconsistentRefused capture");
    assert_eq!(cap.coherence, CaptureCoherenceStatus::InconsistentRefused);
    assert!(!cap.epoch.identity_stable);
    assert!(cap.capture.is_none());
    assert!(cap.can_merge_with(&cap).is_err());
}

#[test]
fn mapping_churn_and_new_thread_mark_capture_partial_or_refused() {
    let mut backend = MockBackend::new_stopped(1001, "start:555");
    backend.write_bytes(0x400000, &[0x90; 16]);
    {
        let mut g = backend.inner.lock().unwrap();
        g.read_call_hook = Some(Box::new(|call, st| {
            if call == 1 {
                // New thread spawned & new mapping appeared mid-acquisition.
                st.state.threads.insert(
                    2,
                    ThreadInfo {
                        thread_id: 2,
                        start_address: 0x500000,
                        suspended: true,
                        is_main: false,
                    },
                );
                st.modules.push(ProcessModule {
                    path: "/lib/churn.so".to_string(),
                    name: "churn.so".to_string(),
                    device: None,
                    inode: None,
                    mappings: vec![ProcessMemoryMapping {
                        runtime_start: 0x500000,
                        runtime_end: 0x501000,
                        file_offset: Some(0),
                        permissions: Some("r-xp".to_string()),
                        path: Some("/lib/churn.so".to_string()),
                    }],
                    address_transform: ModuleAddressTransform::Unavailable {
                        reason: "churn".into(),
                    },
                });
            }
        }));
    }

    let partial = backend
        .acquire_epoch_bound_memory(0x400000, 16, LiveAcquisitionOptions::default())
        .expect("partial");
    assert_eq!(partial.coherence, CaptureCoherenceStatus::Partial);
    assert!(partial.epoch.identity_stable);
    assert!(partial.epoch.threads_changed);
    assert!(partial.epoch.mappings_changed);
    assert_eq!(partial.epoch.observed_thread_ids_before, vec![1]);
    assert_eq!(partial.epoch.observed_thread_ids_after, vec![1, 2]);
    assert!(partial.capture.is_some());

    // With refuse_on_mapping_churn = true, mapping change is refused.
    let mut backend2 = MockBackend::new_stopped(1001, "start:555");
    backend2.write_bytes(0x400000, &[0x90; 16]);
    backend2.inner.lock().unwrap().read_call_hook = Some(Box::new(|call, st| {
        if call == 1 {
            st.modules[0].mappings[0].runtime_end = 0x403000;
        }
    }));
    let refused = backend2
        .acquire_epoch_bound_memory(
            0x400000,
            16,
            LiveAcquisitionOptions {
                refuse_on_mapping_churn: true,
                ..Default::default()
            },
        )
        .expect("refused");
    assert_eq!(
        refused.coherence,
        CaptureCoherenceStatus::InconsistentRefused
    );
    assert!(refused.capture.is_none());
}

#[test]
fn short_read_and_unmapped_gaps_stay_explicit_and_bounded() {
    let mut backend = MockBackend::new_stopped(2002, "start:22");
    // Only first 6 bytes of a 16-byte request are mapped; next 10 bytes are unmapped.
    backend.write_bytes(0x401000, b"\x55\x48\x89\xe5\x90\xc3");

    let cap = backend
        .acquire_epoch_bound_memory(
            0x401000,
            16,
            LiveAcquisitionOptions {
                sparse: SparseReadOptions::with_max_bytes(16),
                ..Default::default()
            },
        )
        .expect("short read produces partial sparse capture");
    assert_eq!(cap.coherence, CaptureCoherenceStatus::Partial);
    let sparse = cap.capture.expect("sparse payload present");
    assert_eq!(sparse.coverage().observed_bytes, 6);
    assert_eq!(sparse.coverage().missing_bytes, 10);
    assert_eq!(sparse.gaps().count(), 1);
}

#[test]
fn breakpoint_patched_bytes_stay_raw_with_opt_in_logical_view() {
    let mut backend = MockBackend::new_stopped(3003, "start:33");
    // Observed memory has 0xCC where debugger installed a software breakpoint.
    backend.write_bytes(0x401000, &[0xCC, 0x48, 0x89, 0xe5]);
    backend.inner.lock().unwrap().breakpoints.push(Breakpoint {
        address: 0x401000,
        original_byte: 0x55,
        enabled: true,
        temporary: false,
        kind: BreakpointKind::Software,
        hits: 0,
        condition: None,
    });

    let cap = backend
        .acquire_epoch_bound_memory(0x401000, 4, LiveAcquisitionOptions::default())
        .expect("capture");
    assert_eq!(cap.view_kind, MemoryViewKind::RawObserved);
    assert_eq!(cap.interventions.len(), 1);
    assert!(matches!(
        cap.interventions[0].kind,
        DebuggerInterventionKind::SoftwareBreakpointPatched {
            address: 0x401000,
            original_byte: 0x55,
            enabled: true
        }
    ));
    let raw = cap.capture.as_ref().expect("raw");
    let raw_bytes: Vec<u8> = raw
        .observed_runs()
        .flat_map(|r| r.bytes.iter().copied())
        .collect();
    // Raw capture NEVER silently substitutes 0x55 for observed 0xCC.
    assert_eq!(raw_bytes, vec![0xCC, 0x48, 0x89, 0xe5]);

    // Explicit opt-in logical view restores original byte separately without mutating `cap`.
    let logical = cap
        .derive_logical_unpatched_view()
        .expect("logical view derivation");
    assert_eq!(logical.view_kind, MemoryViewKind::LogicalUnpatched);
    assert_eq!(
        logical.bytes,
        vec![(0x401000, vec![0x55, 0x48, 0x89, 0xe5])]
    );
    assert_eq!(logical.applied_interventions.len(), 1);
}

#[test]
fn stopped_vs_live_authorization_capabilities_and_shared_mappings() {
    // 1. Stopped all observed threads -> still marks shared_mappings_mutable = true!
    let mut stopped = MockBackend::new_stopped(4004, "start:44");
    stopped.write_bytes(0x401000, &[0x90; 4]);
    let cap_stopped = stopped
        .acquire_epoch_bound_memory(0x401000, 4, LiveAcquisitionOptions::default())
        .expect("stopped capture");
    assert_eq!(
        cap_stopped.epoch.consistency,
        CaptureConsistency::StoppedAllObservedThreads
    );
    assert!(
        cap_stopped.epoch.shared_mappings_mutable,
        "stopping target threads must never claim shared mappings immutable"
    );
    assert_eq!(
        cap_stopped.epoch.thread_enumeration,
        EnumerationCompleteness::Partial
    );

    // 2. Running target with allow_live_unstopped = false -> refuses without suspending target!
    let mut running = MockBackend::new_stopped(4005, "start:45");
    running.write_bytes(0x401000, &[0x90; 4]);
    running.inner.lock().unwrap().state.status = DebugStatus::Running;
    running
        .inner
        .lock()
        .unwrap()
        .state
        .threads
        .get_mut(&1)
        .unwrap()
        .suspended = false;

    let err = running
        .acquire_epoch_bound_memory(0x401000, 4, LiveAcquisitionOptions::default())
        .expect_err("live unstopped read must require explicit authorization");
    assert!(err.to_string().contains("allow_live_unstopped"));
    assert_eq!(running.inner.lock().unwrap().pause_called, 0);

    // 3. Running target with allow_live_unstopped = true -> succeeds as LiveBestEffort + Partial.
    let cap_live = running
        .acquire_epoch_bound_memory(
            0x401000,
            4,
            LiveAcquisitionOptions {
                allow_live_unstopped: true,
                ..Default::default()
            },
        )
        .expect("authorized live capture");
    assert_eq!(
        cap_live.epoch.consistency,
        CaptureConsistency::LiveBestEffort
    );
    assert_eq!(cap_live.coherence, CaptureCoherenceStatus::Partial);

    // 4. Unreported MemoryRead capability fails closed.
    let mut no_cap = MockBackend::new_stopped(4006, "start:46");
    no_cap.capabilities = DebugBackendCapabilities::unreported();
    let cap_err = no_cap
        .acquire_epoch_bound_memory(0x401000, 4, LiveAcquisitionOptions::default())
        .expect_err("unreported MemoryRead must fail closed");
    assert!(cap_err.to_string().contains("MemoryRead"));
}

#[test]
fn merge_guard_allows_same_epoch_and_rejects_mismatched_epoch_or_identity() {
    let mut b1 = MockBackend::new_stopped(5001, "start:1");
    b1.write_bytes(0x401000, &[1, 2, 3, 4]);
    b1.write_bytes(0x401010, &[5, 6, 7, 8]);

    let c1 = b1
        .acquire_epoch_bound_memory(
            0x401000,
            4,
            LiveAcquisitionOptions {
                session_id: "s1".into(),
                epoch_id: 7,
                ..Default::default()
            },
        )
        .unwrap();
    let c2 = b1
        .acquire_epoch_bound_memory(
            0x401010,
            4,
            LiveAcquisitionOptions {
                session_id: "s1".into(),
                epoch_id: 7,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(c1.can_merge_with(&c2).is_ok());

    let c_diff_epoch = b1
        .acquire_epoch_bound_memory(
            0x401010,
            4,
            LiveAcquisitionOptions {
                session_id: "s1".into(),
                epoch_id: 8,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(c1.can_merge_with(&c_diff_epoch).is_err());

    let mut b2 = MockBackend::new_stopped(5001, "start:2");
    b2.write_bytes(0x401010, &[5, 6, 7, 8]);
    let c_diff_token = b2
        .acquire_epoch_bound_memory(
            0x401010,
            4,
            LiveAcquisitionOptions {
                session_id: "s1".into(),
                epoch_id: 7,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(c1.can_merge_with(&c_diff_token).is_err());
}
