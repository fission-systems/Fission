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

/// Tool to load a new binary into the current analysis session.
pub struct LoadBinaryTool;

#[async_trait::async_trait]
impl AiTool for LoadBinaryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "load_binary",
            "Load a new binary executable into the current analysis session.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative path to the binary file to load (e.g. 'benchmark/binary/target.exe')."
                    }
                },
                "required": ["path"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, _context_binary: Option<&Path>) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).context("Missing or invalid 'path'")?;
        let path = PathBuf::from(path_str);
        
        if !path.exists() {
            return Ok(format!("Error: File '{}' does not exist.", path_str));
        }
        if !path.is_file() {
            return Ok(format!("Error: '{}' is not a file.", path_str));
        }

        // We just return success here. The actual state modification happens in AiPipeline::send_internal
        Ok(format!("[✓] Successfully loaded binary from '{}'. You can now use disasm, xrefs, and other tools on it.", path_str))
    }
}

/// Tool to decompile a function to C-like pseudocode.
pub struct DecompileTool;

#[async_trait::async_trait]
impl AiTool for DecompileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "decompile",
            "Decompile a function at a specific memory address to C-like pseudocode.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address of the function to decompile (e.g. '0x140001000')."
                    }
                },
                "required": ["addr"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run decompile.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let addr = addr.to_string();
            move || {
                Command::new(current_cli_exe())
                    .arg("decomp")
                    .arg(binary)
                    .arg("--addr")
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

/// Tool to list all discovered functions in the binary.
pub struct ListFunctionsTool;

#[async_trait::async_trait]
impl AiTool for ListFunctionsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "list_functions",
            "List all discovered functions in the currently loaded binary.",
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run list_functions.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("list")
                    .arg(binary)
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

/// Tool to extract strings from the binary.
pub struct StringsTool;

#[async_trait::async_trait]
impl AiTool for StringsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "strings",
            "Extract printable strings embedded in the binary.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "min_len": {
                        "type": "integer",
                        "description": "Minimum string length (default 6)."
                    }
                }
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run strings.")?;
        let min_len = args.get("min_len").and_then(|v| v.as_u64()).unwrap_or(6);

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("strings")
                    .arg(binary)
                    .arg("--min-len")
                    .arg(min_len.to_string())
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

/// Tool to retrieve basic metadata and inventory of the binary.
pub struct BinaryInfoTool;

#[async_trait::async_trait]
impl AiTool for BinaryInfoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "binary_info",
            "Retrieve basic metadata, architecture, and section info about the loaded binary.",
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run binary_info.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("info")
                    .arg(binary)
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

/// Tool to generate a callgraph for the entire binary.
pub struct CallgraphTool;

#[async_trait::async_trait]
impl AiTool for CallgraphTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "callgraph",
            "Generate caller/callee relationships from xref analysis for the entire binary.",
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run callgraph.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("callgraph")
                    .arg(binary)
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

/// Tool to execute arbitrary Rhai scripts over the Fission binary inventory.
pub struct ScriptTool;

#[async_trait::async_trait]
impl AiTool for ScriptTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "run_script",
            "Execute an arbitrary Rhai script over the Fission binary inventory. Use this for complex, custom programmatic queries.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "script_content": {
                        "type": "string",
                        "description": "The full source code of the Rhai script to execute."
                    }
                },
                "required": ["script_content"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run script.")?;
        let script_content = args.get("script_content").and_then(|v| v.as_str()).context("Missing or invalid 'script_content'")?;

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("fission_ai_script_{}.rhai", std::process::id()));
        tokio::fs::write(&script_path, script_content).await?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let script_path_clone = script_path.clone();
            move || {
                Command::new(current_cli_exe())
                    .arg("script")
                    .arg("run")
                    .arg(binary)
                    .arg("--script")
                    .arg(script_path_clone)
                    .output()
            }
        }).await??;

        let _ = tokio::fs::remove_file(&script_path).await; // Clean up temp file

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!("Error: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }
}

/// Tool to emit raw Rust-Sleigh p-code for a function.
pub struct RawPcodeTool;

#[async_trait::async_trait]
impl AiTool for RawPcodeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "raw_pcode",
            "Emit the Rust-Sleigh raw p-code for a function at a specific memory address.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address of the function to emit raw p-code for (e.g. '0x140001000')."
                    }
                },
                "required": ["addr"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run raw_pcode.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let addr = addr.to_string();
            move || {
                Command::new(current_cli_exe())
                    .arg("raw-pcode")
                    .arg(binary)
                    .arg("--addr")
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

/// Tool to emit raw p-code CFG topology diagnostics for a function.
pub struct PcodeTopologyTool;

#[async_trait::async_trait]
impl AiTool for PcodeTopologyTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "pcode_topology",
            "Emit raw p-code CFG/topology diagnostics for a function.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address of the function to emit topology for (e.g. '0x140001000')."
                    }
                },
                "required": ["addr"]
            })
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run pcode_topology.")?;
        let addr = args.get("addr").and_then(|v| v.as_str()).context("Missing or invalid 'addr'")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            let addr = addr.to_string();
            move || {
                Command::new(current_cli_exe())
                    .arg("pcode-topology")
                    .arg(binary)
                    .arg("--addr")
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
