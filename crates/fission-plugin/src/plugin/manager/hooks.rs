use std::panic::{AssertUnwindSafe, catch_unwind};

use super::core::PluginManager;
use crate::events::{FissionEvent, FissionEventType, format_panic_payload};
use crate::plugin::api::create_binary_info;
use crate::plugin::{
    FailurePolicy, HookDeliveryRecord, HookDispatchReport, HookPriority, PluginDeliveryRecord,
    PluginHook, SubscriberOutcome,
};

impl PluginManager {
    pub fn register_hook<F>(
        &mut self,
        plugin_id: &str,
        event_type: FissionEventType,
        priority: HookPriority,
        callback: F,
    ) -> Result<u64, String>
    where
        F: Fn(&FissionEvent) + Send + Sync + 'static,
    {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found", plugin_id))?;

        let hook_id = self.next_hook_id;
        self.next_hook_id += 1;

        let hook = PluginHook {
            id: hook_id,
            plugin_id: plugin_id.to_string(),
            event_type,
            priority,
        };

        plugin.hooks.push(hook_id);
        self.hooks.insert(hook_id, (hook, Box::new(callback)));

        Ok(hook_id)
    }

    pub fn unregister_hook(&mut self, hook_id: u64) -> Result<(), String> {
        let (hook, _) = self
            .hooks
            .remove(&hook_id)
            .ok_or_else(|| format!("Hook {} not found", hook_id))?;

        if let Some(plugin) = self.plugins.get_mut(&hook.plugin_id) {
            plugin.hooks.retain(|&id| id != hook_id);
        }

        Ok(())
    }

    pub fn emit_event(&self, event: &FissionEvent) {
        let _ = self.emit_event_with_policy(event, FailurePolicy::ContinueOnUnwind);
    }

    pub fn emit_event_checked(&self, event: &FissionEvent) -> HookDispatchReport {
        self.emit_event_with_policy(event, FailurePolicy::ContinueOnUnwind)
    }

    pub fn emit_event_with_policy(
        &self,
        event: &FissionEvent,
        failure_policy: FailurePolicy,
    ) -> HookDispatchReport {
        let event_type = event.event_type();
        let mut plugin_outcomes = Vec::new();
        let mut stop_remaining = false;
        let mut complete = true;

        if let Some(api) = &self.api {
            let mut active_plugins: Vec<_> = self
                .plugins
                .values()
                .filter(|plugin| plugin.info.enabled && plugin.instance.is_some())
                .collect();
            active_plugins.sort_by(|a, b| a.info.id.cmp(&b.info.id));

            for plugin in active_plugins {
                let plugin_id = plugin.info.id.clone();
                if stop_remaining {
                    plugin_outcomes.push(PluginDeliveryRecord {
                        plugin_id,
                        outcome: SubscriberOutcome::SkippedAfterFailure,
                    });
                    continue;
                }

                let Some(instance) = &plugin.instance else {
                    continue;
                };
                let ctx = self.context_for_plugin(api, &plugin.info.id);

                let dispatch_instance = || match event {
                    FissionEvent::BinaryLoaded(binary) => {
                        let info = create_binary_info(binary.as_ref());
                        instance.on_binary_loaded(&ctx, &info);
                    }
                    FissionEvent::DecompilationSuccess { address, code, .. } => {
                        instance.on_function_decompiled(&ctx, *address, code);
                    }
                    _ => {}
                };

                if !matches!(
                    event,
                    FissionEvent::BinaryLoaded(_) | FissionEvent::DecompilationSuccess { .. }
                ) {
                    continue;
                }

                if failure_policy == FailurePolicy::PropagatePanic {
                    dispatch_instance();
                    plugin_outcomes.push(PluginDeliveryRecord {
                        plugin_id,
                        outcome: SubscriberOutcome::Delivered,
                    });
                    continue;
                }

                match catch_unwind(AssertUnwindSafe(dispatch_instance)) {
                    Ok(()) => plugin_outcomes.push(PluginDeliveryRecord {
                        plugin_id,
                        outcome: SubscriberOutcome::Delivered,
                    }),
                    Err(payload) => {
                        complete = false;
                        plugin_outcomes.push(PluginDeliveryRecord {
                            plugin_id,
                            outcome: SubscriberOutcome::Panicked {
                                message: format_panic_payload(&*payload),
                            },
                        });
                        if failure_policy == FailurePolicy::StopOnFirstFailure {
                            stop_remaining = true;
                        }
                    }
                }
            }
        }

        let mut matching_hooks: Vec<_> = self
            .hooks
            .values()
            .filter(|(hook, _)| {
                // A hook runs only for a plugin that is loaded and enabled. A
                // plugin that is no longer in the map leaves an orphan hook.
                match self.plugins.get(&hook.plugin_id) {
                    Some(plugin) if plugin.info.enabled => {}
                    _ => return false,
                }

                hook.event_type == event_type || hook.event_type == FissionEventType::All
            })
            .collect();

        // Deterministic ordering: priority ascending, then stable hook registration ID.
        matching_hooks.sort_by_key(|(hook, _)| (hook.priority, hook.id));

        let mut hook_outcomes = Vec::with_capacity(matching_hooks.len());
        for (hook, callback) in matching_hooks {
            if stop_remaining {
                hook_outcomes.push(HookDeliveryRecord {
                    hook_id: hook.id,
                    plugin_id: hook.plugin_id.clone(),
                    priority: hook.priority,
                    outcome: SubscriberOutcome::SkippedAfterFailure,
                });
                continue;
            }

            if failure_policy == FailurePolicy::PropagatePanic {
                callback(event);
                hook_outcomes.push(HookDeliveryRecord {
                    hook_id: hook.id,
                    plugin_id: hook.plugin_id.clone(),
                    priority: hook.priority,
                    outcome: SubscriberOutcome::Delivered,
                });
                continue;
            }

            match catch_unwind(AssertUnwindSafe(|| callback(event))) {
                Ok(()) => hook_outcomes.push(HookDeliveryRecord {
                    hook_id: hook.id,
                    plugin_id: hook.plugin_id.clone(),
                    priority: hook.priority,
                    outcome: SubscriberOutcome::Delivered,
                }),
                Err(payload) => {
                    complete = false;
                    hook_outcomes.push(HookDeliveryRecord {
                        hook_id: hook.id,
                        plugin_id: hook.plugin_id.clone(),
                        priority: hook.priority,
                        outcome: SubscriberOutcome::Panicked {
                            message: format_panic_payload(&*payload),
                        },
                    });
                    if failure_policy == FailurePolicy::StopOnFirstFailure {
                        stop_remaining = true;
                    }
                }
            }
        }

        HookDispatchReport {
            event_type,
            plugin_outcomes,
            hook_outcomes,
            complete,
        }
    }
}
