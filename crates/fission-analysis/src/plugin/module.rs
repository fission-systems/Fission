use crate::app::modules::{FissionModule, ModuleContext};
use crate::plugin::PluginManager;
use crate::prelude::*;
use std::sync::{Arc, Mutex};

/// Module responsible for the Plugin System
pub struct PluginModule {
    manager: Arc<Mutex<PluginManager>>,
}

impl PluginModule {
    pub fn new(manager: Arc<Mutex<PluginManager>>) -> Self {
        Self { manager }
    }
}

impl FissionModule for PluginModule {
    fn name(&self) -> &str {
        "PluginModule"
    }

    fn on_init(&mut self, ctx: &mut ModuleContext) -> Result<()> {
        // Register PluginManager as a service so others can use it
        ctx.register_service("PluginManager", self.manager.clone());

        let mut mgr = self
            .manager
            .lock()
            .map_err(|e| FissionError::Plugin(format!("Failed to lock plugin manager: {}", e)))?;
        // Inject dependencies using set_event_bus which we added earlier
        mgr.set_event_bus(ctx.event_bus.clone());

        Ok(())
    }

    fn on_start(&mut self, ctx: &mut ModuleContext) -> Result<()> {
        ctx.event_bus
            .publish(crate::app::events::FissionEvent::LogMessage {
                level: "info".into(),
                message: "Plugin System Started".into(),
                target: "PluginModule".into(),
            });
        Ok(())
    }

    fn on_shutdown(&mut self, _ctx: &mut ModuleContext) -> Result<()> {
        // Here we could trigger unloading of all plugins
        Ok(())
    }
}
