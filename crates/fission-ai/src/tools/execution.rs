use super::ToolDefinition;
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub(crate) fn parse_addr(addr: &str) -> Result<u64> {
    let clean_addr = addr.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(clean_addr, 16)
        .ok()
        .or_else(|| addr.parse::<u64>().ok())
        .context("Invalid memory address format. Must be hex or decimal.")
}

pub(crate) fn extract_function_name(code: &str, addr: u64) -> String {
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }
        if let Some(pos) = trimmed.find('(') {
            let before_paren = &trimmed[..pos];
            if let Some(name) = before_paren.split_whitespace().last() {
                let clean_name = name.trim_start_matches('*').to_string();
                if !clean_name.is_empty()
                    && clean_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    return clean_name;
                }
            }
        }
    }
    format!("func_{:#x}", addr)
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run disasm.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;
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
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run xrefs.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;

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
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot apply patch.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'action'")?;
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'value'")?;

        let parsed_addr = parse_addr(addr)?;

        let sidecar_path = binary.with_extension("fission.json");

        // Load existing or initialize new sidecar project
        let mut project = if sidecar_path.exists() {
            let content = std::fs::read_to_string(&sidecar_path)?;
            serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or_else(|_| serde_json::json!({}))
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
            if let Some(names) = project
                .get_mut("user_function_names")
                .and_then(|n| n.as_object_mut())
            {
                names.insert(parsed_addr.to_string(), serde_json::json!(value));
            }
        } else {
            return Ok(format!("Error: Unknown patch action '{}'", action));
        }

        // Fill basic metadata
        if let Some(obj) = project.as_object_mut() {
            if obj.get("binary_path").is_none() {
                obj.insert(
                    "binary_path".to_string(),
                    serde_json::json!(binary.display().to_string()),
                );
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
                        "description": "The absolute or relative path to the binary file to load (e.g. 'test_corpus/target.exe')."
                    }
                },
                "required": ["path"]
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, _context_binary: Option<&Path>) -> Result<String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'path'")?;
        let path = PathBuf::from(path_str);

        if !path.exists() {
            return Ok(format!("Error: File '{}' does not exist.", path_str));
        }
        if !path.is_file() {
            return Ok(format!("Error: '{}' is not a file.", path_str));
        }

        // We just return success here. The actual state modification happens in AiPipeline::send_internal
        Ok(format!(
            "[✓] Successfully loaded binary from '{}'. You can now use disasm, xrefs, and other tools on it.",
            path_str
        ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run decompile.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;

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
        })
        .await??;

        if output.status.success() {
            let decomp_code = String::from_utf8_lossy(&output.stdout).into_owned();
            if !decomp_code.starts_with("Error") {
                if let Ok(parsed_addr) = parse_addr(addr) {
                    let sidecar_path = binary.with_extension("fission.json");
                    let mut project = if sidecar_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&sidecar_path) {
                            serde_json::from_str::<serde_json::Value>(&content)
                                .unwrap_or_else(|_| serde_json::json!({}))
                        } else {
                            serde_json::json!({})
                        }
                    } else {
                        serde_json::json!({})
                    };

                    let mut name = extract_function_name(&decomp_code, parsed_addr);
                    if let Some(user_names) = project
                        .get("user_function_names")
                        .and_then(|n| n.as_object())
                    {
                        if let Some(n) = user_names
                            .get(&parsed_addr.to_string())
                            .and_then(|v| v.as_str())
                        {
                            name = n.to_string();
                        }
                    }

                    if project.get("decompilation_cache").is_none() {
                        if let Some(obj) = project.as_object_mut() {
                            obj.insert("decompilation_cache".to_string(), serde_json::json!({}));
                        }
                    }

                    if let Some(cache) = project
                        .get_mut("decompilation_cache")
                        .and_then(|c| c.as_object_mut())
                    {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        cache.insert(
                            parsed_addr.to_string(),
                            serde_json::json!({
                                "name": name,
                                "code": decomp_code,
                                "timestamp": timestamp
                            }),
                        );
                    }

                    if let Some(obj) = project.as_object_mut() {
                        if obj.get("binary_path").is_none() {
                            obj.insert(
                                "binary_path".to_string(),
                                serde_json::json!(binary.display().to_string()),
                            );
                        }
                    }

                    if let Ok(pretty) = serde_json::to_string_pretty(&project) {
                        let _ = std::fs::write(&sidecar_path, pretty);
                    }
                }
            }
            Ok(decomp_code)
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run list_functions.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("list")
                    .arg(binary)
                    .output()
            }
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
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
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run binary_info.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("info")
                    .arg(binary)
                    .output()
            }
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, _args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run callgraph.")?;

        let output = tokio::task::spawn_blocking({
            let binary = binary.to_path_buf();
            move || {
                Command::new(current_cli_exe())
                    .arg("callgraph")
                    .arg(binary)
                    .output()
            }
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary = context_binary.context("No binary context available. Cannot run script.")?;
        let script_content = args
            .get("script_content")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'script_content'")?;

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
        })
        .await??;

        let _ = tokio::fs::remove_file(&script_path).await; // Clean up temp file

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run raw_pcode.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;

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
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run pcode_topology.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;

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
        })
        .await??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(format!(
                "Error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}

/// Tool to write/update analysis notes for a function.
pub struct AnnotateFunctionTool;

#[async_trait::async_trait]
impl AiTool for AnnotateFunctionTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "annotate_function",
            "Record or update analysis notes (annotations/summary) for a specific function address. These notes are stored persistently in the memory index and are searchable.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {
                        "type": "string",
                        "description": "The memory address of the function to annotate (e.g. '0x140001000')."
                    },
                    "notes": {
                        "type": "string",
                        "description": "The analysis summary, findings, or annotations to store (supports markdown)."
                    }
                },
                "required": ["addr", "notes"]
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run annotate_function.")?;
        let addr = args
            .get("addr")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'addr'")?;
        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'notes'")?;

        let parsed_addr = parse_addr(addr)?;
        let sidecar_path = binary.with_extension("fission.json");
        let mut project = if sidecar_path.exists() {
            let content = std::fs::read_to_string(&sidecar_path)?;
            serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if project.get("annotations").is_none() {
            if let Some(obj) = project.as_object_mut() {
                obj.insert("annotations".to_string(), serde_json::json!({}));
            }
        }

        if let Some(annotations) = project
            .get_mut("annotations")
            .and_then(|a| a.as_object_mut())
        {
            annotations.insert(parsed_addr.to_string(), serde_json::json!(notes));
        }

        if let Some(obj) = project.as_object_mut() {
            if obj.get("binary_path").is_none() {
                obj.insert(
                    "binary_path".to_string(),
                    serde_json::json!(binary.display().to_string()),
                );
            }
        }

        let pretty = serde_json::to_string_pretty(&project)?;
        std::fs::write(&sidecar_path, pretty)?;

        Ok(format!(
            "[✓] Successfully saved analysis annotation for function at address {:#x}.\nThese notes are now persistent and searchable in the memory index.",
            parsed_addr
        ))
    }
}

/// Tool to search the persistent memory index.
pub struct SearchMemoryTool;

#[async_trait::async_trait]
impl AiTool for SearchMemoryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "search_memory",
            "Search the persistent analysis memory index for previously decompiled functions or annotations containing the query string (case-insensitive).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search term or pattern to look for (e.g. 'main', 'secret_key', '0x401000')."
                    }
                },
                "required": ["query"]
            }),
        )
    }

    async fn execute(&self, args: &JsonValue, context_binary: Option<&Path>) -> Result<String> {
        let binary =
            context_binary.context("No binary context available. Cannot run search_memory.")?;
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .context("Missing or invalid 'query'")?;
        let query_lower = query.to_lowercase();

        let sidecar_path = binary.with_extension("fission.json");
        if !sidecar_path.exists() {
            return Ok("[!] No analysis memory index has been created for this binary yet. Try decompiling a function first.".to_string());
        }

        let content = std::fs::read_to_string(&sidecar_path)?;
        let project: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}));

        let decomp_cache = project
            .get("decompilation_cache")
            .and_then(|c| c.as_object());
        let annotations = project.get("annotations").and_then(|a| a.as_object());

        let mut matches = std::collections::HashSet::new();

        if let Some(cache) = decomp_cache {
            for (addr_str, val) in cache {
                let name = val.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let code = val.get("code").and_then(|c| c.as_str()).unwrap_or("");

                let parsed_addr = addr_str.parse::<u64>().unwrap_or(0);
                let hex_addr = format!("{:#x}", parsed_addr);

                if addr_str.contains(&query_lower)
                    || hex_addr.to_lowercase().contains(&query_lower)
                    || name.to_lowercase().contains(&query_lower)
                    || code.to_lowercase().contains(&query_lower)
                {
                    matches.insert(addr_str.clone());
                }
            }
        }

        if let Some(ann) = annotations {
            for (addr_str, val) in ann {
                let notes = val.as_str().unwrap_or("");
                let parsed_addr = addr_str.parse::<u64>().unwrap_or(0);
                let hex_addr = format!("{:#x}", parsed_addr);

                if addr_str.contains(&query_lower)
                    || hex_addr.to_lowercase().contains(&query_lower)
                    || notes.to_lowercase().contains(&query_lower)
                {
                    matches.insert(addr_str.clone());
                }
            }
        }

        if matches.is_empty() {
            return Ok(format!(
                "No matches found in the memory index for query '{}'.",
                query
            ));
        }

        let mut output = format!(
            "### Search Results for '{}' (Found {} matches)\n\n",
            query,
            matches.len()
        );

        for addr_str in matches {
            let parsed_addr = addr_str.parse::<u64>().unwrap_or(0);
            let hex_addr = format!("{:#x}", parsed_addr);

            let name = decomp_cache
                .and_then(|c| c.get(&addr_str))
                .and_then(|v| v.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");

            let code = decomp_cache
                .and_then(|c| c.get(&addr_str))
                .and_then(|v| v.get("code"))
                .and_then(|c| c.as_str());

            let note_str = annotations
                .and_then(|a| a.get(&addr_str))
                .and_then(|n| n.as_str())
                .unwrap_or("*(No notes recorded)*");

            output.push_str(&format!("#### Function: `{}` ({})\n", name, hex_addr));
            output.push_str(&format!("**AI Notes:**\n{}\n\n", note_str));

            if let Some(code_text) = code {
                let mut snippet = String::new();
                let lines: Vec<&str> = code_text.lines().collect();
                let mut found_line = None;

                for (i, line) in lines.iter().enumerate() {
                    if line.to_lowercase().contains(&query_lower) {
                        found_line = Some(i);
                        break;
                    }
                }

                if let Some(line_idx) = found_line {
                    let start = line_idx.saturating_sub(2);
                    let end = std::cmp::min(line_idx + 3, lines.len());
                    snippet.push_str("```c\n// ... matching context snippet ...\n");
                    for i in start..end {
                        let prefix = if i == line_idx { "> " } else { "  " };
                        snippet.push_str(&format!("{}{}\n", prefix, lines[i]));
                    }
                    snippet.push_str("```\n\n");
                } else {
                    let preview_limit = std::cmp::min(5, lines.len());
                    snippet.push_str("```c\n// ... function preview ...\n");
                    for line in lines.iter().take(preview_limit) {
                        snippet.push_str(&format!("  {}\n", line));
                    }
                    snippet.push_str("```\n\n");
                }
                output.push_str(&snippet);
            }
            output.push_str("---\n\n");
        }

        Ok(output)
    }
}
