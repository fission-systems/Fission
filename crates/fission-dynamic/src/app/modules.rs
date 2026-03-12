use crate::app::events::EventBus;
use crate::prelude::*;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

/// Module lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    Uninitialized,
    Initialized,
    Running,
    Stopped,
    Failed,
}

/// Context provided to modules during lifecycle events
pub struct ModuleContext {
    pub event_bus: Arc<EventBus>,
    // Service locator pattern for cross-module dependencies
    pub services: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl ModuleContext {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            services: HashMap::new(),
        }
    }

    pub fn register_service<T: Any + Send + Sync>(&mut self, name: &str, service: Arc<T>) {
        self.services.insert(name.to_string(), service);
    }

    pub fn get_service<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        self.services
            .get(name)
            .and_then(|any| any.downcast_ref::<Arc<T>>().cloned())
    }
}

/// Trait defining a Fission Module
pub trait FissionModule: Send + Sync {
    /// Unique name of the module
    fn name(&self) -> &str;

    /// Initialization phase: Register services, load config
    fn on_init(&mut self, ctx: &mut ModuleContext) -> Result<()>;

    /// Start phase: Start background threads, active logic
    fn on_start(&mut self, ctx: &mut ModuleContext) -> Result<()>;

    /// Shutdown phase: Cleanup resources
    fn on_shutdown(&mut self, ctx: &mut ModuleContext) -> Result<()>;
}

/// Wrapper to track module state
struct ModuleEntry {
    module: Box<dyn FissionModule>,
    state: ModuleState,
}

/// Manager for all modules with lifecycle tracking
pub struct ModuleManager {
    modules: Vec<ModuleEntry>,
    context: ModuleContext,
    manager_state: ModuleState,
}

impl ModuleManager {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            modules: Vec::new(),
            context: ModuleContext::new(event_bus),
            manager_state: ModuleState::Uninitialized,
        }
    }

    pub fn register_module(&mut self, module: Box<dyn FissionModule>) {
        self.modules.push(ModuleEntry {
            module,
            state: ModuleState::Uninitialized,
        });
    }

    /// Initialize all modules with graceful degradation
    pub fn init_all(&mut self) -> Result<()> {
        let mut all_ok = true;
        for entry in &mut self.modules {
            match entry.module.on_init(&mut self.context) {
                Ok(()) => {
                    entry.state = ModuleState::Initialized;
                    crate::core::logging::info(&format!(
                        "[ModuleManager] {} initialized",
                        entry.module.name()
                    ));
                }
                Err(e) => {
                    entry.state = ModuleState::Failed;
                    crate::core::logging::error(&format!(
                        "[ModuleManager] {} failed to initialize: {}",
                        entry.module.name(),
                        e
                    ));
                    all_ok = false;
                    // Continue with other modules (graceful degradation)
                }
            }
        }
        self.manager_state = ModuleState::Initialized;
        if all_ok { Ok(()) } else { Ok(()) } // Log errors but don't fail startup
    }

    /// Start all initialized modules with graceful degradation
    pub fn start_all(&mut self) -> Result<()> {
        for entry in &mut self.modules {
            if entry.state != ModuleState::Initialized {
                continue; // Skip failed or already running modules
            }
            match entry.module.on_start(&mut self.context) {
                Ok(()) => {
                    entry.state = ModuleState::Running;
                    crate::core::logging::info(&format!(
                        "[ModuleManager] {} started",
                        entry.module.name()
                    ));
                }
                Err(e) => {
                    entry.state = ModuleState::Failed;
                    crate::core::logging::error(&format!(
                        "[ModuleManager] {} failed to start: {}",
                        entry.module.name(),
                        e
                    ));
                    // Continue with other modules
                }
            }
        }
        self.manager_state = ModuleState::Running;
        Ok(())
    }

    /// Shutdown all running modules in reverse order
    pub fn shutdown_all(&mut self) -> Result<()> {
        for entry in self.modules.iter_mut().rev() {
            if entry.state != ModuleState::Running {
                continue;
            }
            match entry.module.on_shutdown(&mut self.context) {
                Ok(()) => {
                    entry.state = ModuleState::Stopped;
                    crate::core::logging::info(&format!(
                        "[ModuleManager] {} stopped",
                        entry.module.name()
                    ));
                }
                Err(e) => {
                    crate::core::logging::warn(&format!(
                        "[ModuleManager] {} failed to shutdown cleanly: {}",
                        entry.module.name(),
                        e
                    ));
                    entry.state = ModuleState::Stopped; // Mark stopped anyway
                }
            }
        }
        self.manager_state = ModuleState::Stopped;
        Ok(())
    }

    /// Get overall manager state
    pub fn state(&self) -> ModuleState {
        self.manager_state
    }

    /// Get context for service lookup
    pub fn context(&self) -> &ModuleContext {
        &self.context
    }
}
