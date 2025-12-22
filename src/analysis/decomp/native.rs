//! Subprocess-based Ghidra Decompiler interface.
//!
//! Spawns `fission_decomp` CLI for each decompilation request to avoid
//! Ghidra global state issues that cause crashes on subsequent calls.

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
    pub fn decompile(&self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Result<String> {
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
        
        // Check for errors in stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() && stderr.contains("error") {
            return Err(anyhow!("Decompiler error: {}", stderr.trim()));
        }
        
        // Return stdout as result
        let result = String::from_utf8_lossy(&output.stdout).to_string();
        if result.is_empty() {
            return Err(anyhow!("Decompiler produced no output"));
        }
        
        Ok(result)
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
