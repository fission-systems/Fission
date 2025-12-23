//! Subprocess-based Ghidra Decompiler interface.
//!
//! Spawns `fission_decomp` CLI for each decompilation request.

use std::process::{Command, Stdio};
use std::io::Write;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use anyhow::{anyhow, Result};

/// Subprocess-based decompiler interface
pub struct NativeDecompiler {
    cli_path: std::path::PathBuf,
    sla_dir: String,
}

impl NativeDecompiler {
    /// Create new decompiler with path to CLI binary and SLA directory
    pub fn new<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str) -> Result<Self> {
        let path = cli_path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(anyhow!("Decompiler CLI not found: {:?}", path));
        }
        Ok(Self {
            cli_path: path,
            sla_dir: sla_dir.to_string(),
        })
    }

    /// Decompile bytes by spawning subprocess
    pub fn decompile(&mut self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Result<String> {
        // Encode bytes as base64
        let bytes_b64 = BASE64.encode(bytes);
        
        // Create JSON input
        let input = format!(
            r#"{{"bytes":"{}","address":{},"is_64bit":{},"sla_dir":"{}"}}"#,
            bytes_b64,
            base_addr,
            if is_64bit { "true" } else { "false" },
            self.sla_dir
        );
        
        // Spawn subprocess
        let mut child = Command::new(&self.cli_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn decompiler: {}", e))?;
        
        // Write input to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())
                .map_err(|e| anyhow!("Failed to write to decompiler stdin: {}", e))?;
        }
        
        // Wait for completion and get output
        let output = child.wait_with_output()
            .map_err(|e| anyhow!("Failed to wait for decompiler: {}", e))?;
        
        // Capture stderr for error messages
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Parse stdout for JSON response FIRST (process may crash on cleanup but still produce valid output)
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        
        // Try to extract result even if process crashed during cleanup
        if stdout.contains("\"status\":\"ok\"") {
            if let Some(start) = stdout.find("\"code\":\"") {
                let start = start + 8;
                let chars: Vec<char> = stdout.chars().collect();
                let mut end = start;
                while end < chars.len() {
                    if chars[end] == '"' && (end == start || chars[end - 1] != '\\') {
                        break;
                    }
                    end += 1;
                }
                let code = &stdout[start..end];
                let code = code
                    .replace("\\n", "\n")
                    .replace("\\r", "\r")
                    .replace("\\t", "\t")
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\");
                return Ok(code);
            }
        }
        
        // Check for error status in JSON
        if stdout.contains("\"status\":\"error\"") {
            if let Some(start) = stdout.find("\"message\":\"") {
                let start = start + 11;
                if let Some(end) = stdout[start..].find("\"") {
                    let msg = &stdout[start..start + end];
                    return Err(anyhow!("Decompiler error: {}", msg));
                }
            }
            return Err(anyhow!("Decompiler error: {}", stdout));
        }
        
        // Check exit code only if no valid JSON output
        if !output.status.success() {
            return Err(anyhow!("Decompiler failed (exit {}): {}", 
                output.status.code().unwrap_or(-1), stderr.trim()));
        }
        
        // Fallback: return raw stdout (old format compatibility)
        if !stdout.is_empty() && !stdout.starts_with("{") {
            return Ok(stdout);
        }
        
        Err(anyhow!("Invalid response format: {}", stdout))
    }
}

/// Helper to find the decompiler CLI in the project structure
pub fn find_cli() -> Option<std::path::PathBuf> {
    let base = std::env::current_dir().ok()?;
    
    #[cfg(target_os = "windows")]
    let cli_name = "fission_decomp.exe";
    
    #[cfg(not(target_os = "windows"))]
    let cli_name = "fission_decomp";

    let paths = [
        base.join(cli_name),
        base.join("build/Release").join(cli_name),
        base.join("build/Debug").join(cli_name),
        base.join("build").join(cli_name),
        base.join("ghidra_decompiler/build").join(cli_name),
    ];

    for p in &paths {
        if p.exists() {
            return Some(p.clone());
        }
    }
    None
}

// Keep old function name for compatibility
pub fn find_library() -> Option<std::path::PathBuf> {
    find_cli()
}
