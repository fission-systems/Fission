#![cfg(feature = "interactive_runtime")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

use fission_core::common::types::{BinaryInfo, FunctionInfo};
use fission_plugin::events::{
    DispatchPolicy, EventBus, EventPriority, FailurePolicy, FissionEvent, FissionEventType,
    ReentrantPolicy, ReentrantStatus, SubscriberOutcome, SubscriptionCriticality,
};
use fission_plugin::plugin::{
    FissionPlugin, HookPriority, PluginAPI, PluginContext, PluginManager,
};

struct DummyApi;

impl PluginAPI for DummyApi {
    fn get_binary(&self) -> Option<BinaryInfo> {
        None
    }
    fn get_functions(&self) -> Vec<FunctionInfo> {
        Vec::new()
    }
    fn read_binary_bytes(&self, _address: u64, _size: usize) -> Option<Vec<u8>> {
        None
    }
    fn log(&self, _message: &str) {}
    fn log_error(&self, _message: &str) {}
    fn decompile(&self, _address: u64) -> Option<String> {
        None
    }
    fn get_current_decompiled_code(&self) -> Option<String> {
        None
    }
    fn disassemble(&self, _address: u64, _size: usize) -> Vec<String> {
        Vec::new()
    }
}

struct RecordingPlugin {
    id: String,
    should_panic: bool,
    log: Arc<Mutex<Vec<String>>>,
}

impl FissionPlugin for RecordingPlugin {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.id
    }

    fn on_function_decompiled(&self, _ctx: &PluginContext, addr: u64, _code: &str) {
        self.log
            .lock()
            .unwrap()
            .push(format!("plugin:{}:{addr:#x}", self.id));
        if self.should_panic {
            panic!("plugin {} intentional unwind", self.id);
        }
    }
}

#[test]
fn event_bus_deterministic_priority_and_registration_order() {
    let bus = EventBus::new();
    let order = Arc::new(Mutex::new(Vec::new()));

    let register = |prio: EventPriority, label: &'static str| {
        let log = order.clone();
        bus.subscribe_with_priority(prio, move |_| {
            log.lock().unwrap().push(label);
        })
    };

    let id_low_1 = register(EventPriority::Low, "low-1");
    let id_norm_1 = register(EventPriority::Normal, "norm-1");
    let id_high_1 = register(EventPriority::High, "high-1");
    let id_norm_2 = register(EventPriority::Normal, "norm-2");
    let id_high_2 = register(EventPriority::High, "high-2");
    let id_low_2 = register(EventPriority::Low, "low-2");

    for _ in 0..5 {
        order.lock().unwrap().clear();
        let report = bus.publish_checked(FissionEvent::AppStarted);
        assert!(report.complete);
        assert_eq!(
            *order.lock().unwrap(),
            vec!["high-1", "high-2", "norm-1", "norm-2", "low-1", "low-2"]
        );
        let ids: Vec<u64> = report.records.iter().map(|r| r.subscription_id).collect();
        assert_eq!(
            ids,
            vec![
                id_high_1, id_high_2, id_norm_1, id_norm_2, id_low_1, id_low_2
            ]
        );
    }
}

#[test]
fn event_bus_isolates_independent_panic_and_supports_stop_and_critical_policies() {
    // 1. ContinueOnUnwind: failing middle subscriber does not drop subsequent subscriber.
    let bus = EventBus::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    let l1 = log.clone();
    bus.subscribe(move |_| l1.lock().unwrap().push("first"));
    bus.subscribe(move |_| panic!("recoverable subscriber panic"));
    let l3 = log.clone();
    bus.subscribe(move |_| l3.lock().unwrap().push("third"));

    let report = bus.publish_checked(FissionEvent::AppStarted);
    assert!(!report.complete);
    assert_eq!(*log.lock().unwrap(), vec!["first", "third"]);
    assert_eq!(report.records[0].outcome, SubscriberOutcome::Delivered);
    assert!(matches!(
        &report.records[1].outcome,
        SubscriberOutcome::Panicked { message } if message.contains("recoverable subscriber panic")
    ));
    assert_eq!(report.records[2].outcome, SubscriberOutcome::Delivered);

    // 2. StopOnFirstFailure: third subscriber is skipped explicitly.
    log.lock().unwrap().clear();
    let stop_report = bus.publish_with_policy(
        FissionEvent::AppStarted,
        DispatchPolicy {
            failure_policy: FailurePolicy::StopOnFirstFailure,
            ..DispatchPolicy::default()
        },
    );
    assert!(!stop_report.complete);
    assert_eq!(*log.lock().unwrap(), vec!["first"]);
    assert_eq!(
        stop_report.records[2].outcome,
        SubscriberOutcome::SkippedAfterFailure
    );

    // 3. Critical subscriber panic propagates and RAII guard cleans up thread state cleanly.
    let crit_bus = EventBus::new();
    let tail_hits = Arc::new(AtomicUsize::new(0));
    crit_bus.subscribe_with_options(
        EventPriority::High,
        SubscriptionCriticality::Critical,
        |_| panic!("critical transactional observer failed"),
    );
    let tail_c = tail_hits.clone();
    let crit_id = 0_u64;
    let _ = crit_id;
    crit_bus.subscribe(move |_| {
        tail_c.fetch_add(1, Ordering::SeqCst);
    });

    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crit_bus.publish(FissionEvent::AppStarted);
    }));
    assert!(caught.is_err());
    assert_eq!(tail_hits.load(Ordering::SeqCst), 0);

    // After critical unwind, reentrancy frame must be clean (unsubscribe critical sub and publish).
    crit_bus.unsubscribe(0);
    let after = crit_bus.publish_checked(FissionEvent::AppStarted);
    assert!(after.complete);
    assert_eq!(tail_hits.load(Ordering::SeqCst), 1);
}

#[test]
fn event_bus_bounded_reentrant_publish_queue_and_direct_modes() {
    // Mode A: QueueBounded { max_queued: 4 } stops infinite self-recursion cleanly.
    let q_bus = Arc::new(EventBus::new());
    let q_seen = Arc::new(AtomicUsize::new(0));
    let q_rejected = Arc::new(AtomicUsize::new(0));

    let bus_ref = q_bus.clone();
    let seen_ref = q_seen.clone();
    let rej_ref = q_rejected.clone();
    let policy = DispatchPolicy {
        reentrant_policy: ReentrantPolicy::QueueBounded { max_queued: 4 },
        ..DispatchPolicy::default()
    };

    q_bus.subscribe(move |_| {
        seen_ref.fetch_add(1, Ordering::SeqCst);
        let nested = bus_ref.publish_with_policy(FissionEvent::DebugStep { thread_id: 1 }, policy);
        if nested.reentrant_status == ReentrantStatus::RejectedQueueFull {
            rej_ref.fetch_add(1, Ordering::SeqCst);
        }
    });

    let report = q_bus.publish_with_policy(FissionEvent::AppStarted, policy);
    assert_eq!(report.reentrant_drained, 4);
    assert_eq!(report.reentrant_rejected, 1);
    assert!(!report.complete);
    assert_eq!(q_seen.load(Ordering::SeqCst), 5); // 1 outer + 4 queued
    assert_eq!(q_rejected.load(Ordering::SeqCst), 1);

    // Mode B: DirectBounded { max_depth: 3 } executes depths 0, 1, 2 and rejects depth 3.
    let d_bus = Arc::new(EventBus::new());
    let d_seen = Arc::new(AtomicUsize::new(0));
    let d_rejected = Arc::new(AtomicUsize::new(0));
    let d_policy = DispatchPolicy {
        reentrant_policy: ReentrantPolicy::DirectBounded { max_depth: 3 },
        ..DispatchPolicy::default()
    };

    let dbus_ref = d_bus.clone();
    let dseen_ref = d_seen.clone();
    let drej_ref = d_rejected.clone();
    d_bus.subscribe(move |_| {
        dseen_ref.fetch_add(1, Ordering::SeqCst);
        let nested =
            dbus_ref.publish_with_policy(FissionEvent::DebugStep { thread_id: 2 }, d_policy);
        if nested.reentrant_status == ReentrantStatus::RejectedDepthExceeded {
            drej_ref.fetch_add(1, Ordering::SeqCst);
        }
    });

    let d_report = d_bus.publish_with_policy(FissionEvent::AppStarted, d_policy);
    assert_eq!(d_report.reentrant_rejected, 1);
    assert!(!d_report.complete);
    assert_eq!(d_seen.load(Ordering::SeqCst), 3);
    assert_eq!(d_rejected.load(Ordering::SeqCst), 1);
}

#[test]
fn event_bus_subscribe_and_unsubscribe_during_delivery() {
    let bus = Arc::new(EventBus::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    let id2_slot = Arc::new(Mutex::new(None::<u64>));

    let bus_c = bus.clone();
    let log1 = log.clone();
    let slot_c = id2_slot.clone();
    bus.subscribe_with_priority(EventPriority::High, move |_| {
        log1.lock().unwrap().push("sub1");
        if let Some(id2) = *slot_c.lock().unwrap() {
            bus_c.unsubscribe(id2);
        }
        let log3 = log1.clone();
        bus_c.subscribe_with_priority(EventPriority::High, move |_| {
            log3.lock().unwrap().push("sub3_late");
        });
    });

    let log2 = log.clone();
    let id2 = bus.subscribe_with_priority(EventPriority::Low, move |_| {
        log2.lock().unwrap().push("sub2_should_skip");
    });
    *id2_slot.lock().unwrap() = Some(id2);

    let first = bus.publish_checked(FissionEvent::AppStarted);
    assert!(first.complete);
    assert_eq!(*log.lock().unwrap(), vec!["sub1"]);
    assert_eq!(first.records.len(), 2);
    assert_eq!(
        first.records[1].outcome,
        SubscriberOutcome::SkippedUnsubscribed
    );

    log.lock().unwrap().clear();
    let second = bus.publish_checked(FissionEvent::AppStarted);
    assert!(second.complete);
    assert_eq!(*log.lock().unwrap(), vec!["sub1", "sub3_late"]);
}

#[test]
fn event_bus_concurrent_publishers_and_subscribers_complete_without_deadlock() {
    let bus = Arc::new(EventBus::new());
    let hits = Arc::new(AtomicUsize::new(0));

    let h0 = hits.clone();
    bus.subscribe(move |_| {
        h0.fetch_add(1, Ordering::Relaxed);
    });
    bus.subscribe(|_| panic!("noisy concurrent subscriber"));

    let threads = 6usize;
    let barrier = Arc::new(Barrier::new(threads));
    let mut handles = Vec::with_capacity(threads);

    for t in 0..threads {
        let bus_c = bus.clone();
        let hits_c = hits.clone();
        let bar_c = barrier.clone();
        handles.push(std::thread::spawn(move || {
            bar_c.wait();
            for iter in 0..20 {
                let hc = hits_c.clone();
                let temp_id = bus_c.subscribe(move |_| {
                    hc.fetch_add(1, Ordering::Relaxed);
                });
                let rep = bus_c.publish_checked(FissionEvent::DebugStep {
                    thread_id: (t * 100 + iter) as u32,
                });
                assert!(!rep.complete);
                bus_c.unsubscribe(temp_id);
            }
        }));
    }

    for h in handles {
        h.join()
            .expect("thread must not panic under ContinueOnUnwind");
    }
    assert!(hits.load(Ordering::Relaxed) >= threads * 20);
}

#[test]
fn plugin_manager_hooks_deterministic_tie_breaking_and_panic_isolation() {
    let mut pm = PluginManager::new();
    pm.set_api(Arc::new(DummyApi));
    let log = Arc::new(Mutex::new(Vec::new()));

    // Register in reverse alphabetical ID order; instance delivery must sort deterministically.
    pm.register_native_plugin(Box::new(RecordingPlugin {
        id: "zeta".into(),
        should_panic: false,
        log: log.clone(),
    }))
    .expect("register zeta");
    pm.register_native_plugin(Box::new(RecordingPlugin {
        id: "alpha".into(),
        should_panic: true,
        log: log.clone(),
    }))
    .expect("register alpha");

    let push_hook = |pm: &mut PluginManager,
                     plugin: &str,
                     prio: HookPriority,
                     tag: &'static str,
                     panic_flag: bool| {
        let log_c = log.clone();
        pm.register_hook(
            plugin,
            FissionEventType::DecompilationSuccess,
            prio,
            move |_| {
                log_c.lock().unwrap().push(tag.to_string());
                if panic_flag {
                    panic!("hook {tag} unwind");
                }
            },
        )
        .expect("register hook")
    };

    // Equal-priority hooks must execute in ascending hook registration ID order across trials.
    let h_low = push_hook(&mut pm, "zeta", HookPriority::Low, "hook:low", false);
    let h_norm_1 = push_hook(&mut pm, "alpha", HookPriority::Normal, "hook:norm-1", true);
    let h_norm_2 = push_hook(&mut pm, "zeta", HookPriority::Normal, "hook:norm-2", false);
    let h_high = push_hook(&mut pm, "alpha", HookPriority::High, "hook:high", false);

    let event = FissionEvent::DecompilationSuccess {
        address: 0x401000,
        function_name: Some("main".into()),
        code: "int main() {}".into(),
    };

    let report = pm.emit_event_checked(&event);
    assert!(!report.complete);
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "plugin:alpha:0x401000",
            "plugin:zeta:0x401000",
            "hook:high",
            "hook:norm-1",
            "hook:norm-2",
            "hook:low",
        ]
    );
    assert_eq!(report.plugin_outcomes.len(), 2);
    assert!(matches!(
        &report.plugin_outcomes[0].outcome,
        SubscriberOutcome::Panicked { message } if message.contains("alpha intentional unwind")
    ));
    assert_eq!(
        report.plugin_outcomes[1].outcome,
        SubscriberOutcome::Delivered
    );

    let hook_ids: Vec<u64> = report.hook_outcomes.iter().map(|h| h.hook_id).collect();
    assert_eq!(hook_ids, vec![h_high, h_norm_1, h_norm_2, h_low]);
    assert!(matches!(
        &report.hook_outcomes[1].outcome,
        SubscriberOutcome::Panicked { message } if message.contains("hook:norm-1 unwind")
    ));
    assert_eq!(
        report.hook_outcomes[2].outcome,
        SubscriberOutcome::Delivered
    );

    // Under StopOnFirstFailure, alpha plugin panic stops zeta plugin and all hooks.
    log.lock().unwrap().clear();
    let stop_report = pm.emit_event_with_policy(&event, FailurePolicy::StopOnFirstFailure);
    assert!(!stop_report.complete);
    assert_eq!(*log.lock().unwrap(), vec!["plugin:alpha:0x401000"]);
    assert_eq!(
        stop_report.plugin_outcomes[1].outcome,
        SubscriberOutcome::SkippedAfterFailure
    );
    assert!(
        stop_report
            .hook_outcomes
            .iter()
            .all(|h| h.outcome == SubscriberOutcome::SkippedAfterFailure)
    );
}
