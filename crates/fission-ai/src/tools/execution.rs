use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::process::Command;
use super::ToolDefinition;

/// Trait defining a tool that can be executed by the AI.
#[async_trait::async_trait]
pub trait AiTool: Send + Sync {
    /// Returns the schema definition of the tool.
    fn definition(&self) -> ToolDefinition;
    
    /// Executes the tool with the given JSON arguments and binary context.
    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String>;
}

/// Helper to get the current executable so we can re-invoke Fission CLI as a subprocess.
fn current_cli_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "fission_cli".into())
}

/// Tool to disassemble instructions.
pub struct DisasmTool;

#[async_trait::async_trait]
impl AiTool for DisasmTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "disasm",
            "Disassemble instructions around a given memory address.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address to disassemble (e.g. '0x140001000')."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of instructions to disassemble (default 20)."
                    }
                },
                "required": ["addr"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run disasm.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;
        let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(20);

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let addr = addr.to_string();
            move || {
                Command::new(current_cli_exe())
                    .arg("disasm")
                    .arg(binary)
                    .arg("--addr")
                    .arg(addr)
                    .arg("--count")
                    .arg(count.to_string())
                    .output()
            }
        }).await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!("Error: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }
}

/// Tool to get cross-references (xrefs).
pub struct XrefsTool;

#[async_trait::async_trait]
impl AiTool for XrefsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "xrefs",
            "Get cross-references to or from a specific memory address.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address to find xrefs for (e.g. '0x140001000')."
                    }
                },
                "required": ["addr"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run xrefs.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let addr = addr.to_string();
            move || {
                Command::new(current_cli_exe())
                    .arg("xrefs")
                    .arg(binary)
                    .arg("--function")
                    .arg(addr)
                    .output()
            }
        }).await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!("Error: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }
}

/// Tool to apply persistent metadata patches (such as function renaming).
pub struct ApplyPatchTool;

#[async_trait::async_trait]
impl AiTool for ApplyPatchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "apply_patch",
            "Apply a persistent metadata patch (such as renaming a function) for a memory address. This rename will persist and automatically propagate to all future decompiler outputs.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address to patch (e.g. '0x140001000')."
                    },
                    "action": {
                        "type": "string",
                        "enum": ["rename_function"],
                        "description": "The type of metadata patch to apply."
                    },
                    "value": {
                        "type": "string",
                        "description": "The new function name to assign."
                    }
                },
                "required": ["addr", "action", "value"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot apply patch.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;
        let action = args.get("action").and_then(|v| v.as_str()).context("Missing or invalid 'action'")?;
        let value = args.get("value").and_then(|v| v.as_str()).context("Missing or invalid 'value'")?;

        let clean_addr = addr.trim_start_matches("0x").trim_start_matches("0X");
        let parsed_addr = u64::from_str_radix(clean_addr, 16)
            .ok()
            .or_else(|| addr.parse::<u64>().ok())
            .context("Invalid memory address format. Must be hex or decimal.")?;

        let sidecar_path = binary.with_extension("fission.json");

        // Load existing or initialize new sidecar project
        let mut project = if sidecar_path.exists() {
            let content = std::fs::read_to_string(&sidecar_path)?;
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Initialize maps if missing
        if project.get("user_function_names").is_none() {
            if let Some(obj) = project.as_object_mut() {
                obj.insert("user_function_names".to_string(), serde_json::json!({}));
            }
        }

        // Apply action
        if action == "rename_function" {
            if let Some(names) = project.get_mut("user_function_names").and_then(|n| n.as_object_mut()) {
                names.insert(parsed_addr.to_string(), serde_json::json!(value));
            }
        } else {
            return Ok(format!("Error: Unknown patch action '{}'", action));
        }

        // Fill basic metadata
        if let Some(obj) = project.as_object_mut() {
            if obj.get("binary_path").is_none() {
                obj.insert("binary_path".to_string(), serde_json::json!(binary.display().to_string()));
            }
        }

        // Save back
        let pretty = serde_json::to_string_pretty(&project)?;
        std::fs::write(&sidecar_path, pretty)?;

        Ok(format!(
            "[✓] Successfully applied patch: {} to \"{}\" at address {} ({:#x}).\nThis rename will be active in all subsequent decompilations.",
            action, value, parsed_addr, parsed_addr
        ))
    }
}
