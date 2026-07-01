use super::ToolDefinition;
use super::execution::AiTool;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AiTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: AiTool + 'static>(&mut self, tool: T) {
        let def = tool.definition();
        self.tools.insert(def.callable_name, Arc::new(tool));
    }

    pub fn get_model_visible_tools(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn AiTool>> {
        if let Some(tool) = self.tools.get(name) {
            return Some(tool.clone());
        }

        // Handle namespaced tool names (e.g. from GitHub Copilot like `fission__disasm`)
        if let Some((_, suffix)) = name.split_once("__")
            && let Some(tool) = self.tools.get(suffix)
        {
            return Some(tool.clone());
        }

        None
    }
}
