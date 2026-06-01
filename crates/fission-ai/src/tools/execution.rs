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
