//! Subprocess-based Ghidra Decompiler interface.
//!
//! Two modes:
//! - `NativeDecompiler`: Spawns a new process per request (legacy)
//! - `DecompilerServer`: Persistent server process for faster repeated requests

use std::process::{Command, Stdio, Child, ChildStdin, ChildStdout};
use std::io::{Write, BufRead, BufReader};
use std::sync::{Arc, Mutex};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use anyhow::{anyhow, Result};

/// Legacy subprocess-based decompiler (spawns new process each time)
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
        let bytes_b64 = BASE64.encode(bytes);
        
        let input = format!(
            r#"{{"bytes":"{}","address":{},"is_64bit":{},"sla_dir":"{}"}}"#,
            bytes_b64,
            base_addr,
            if is_64bit { "true" } else { "false" },
            self.sla_dir
        );
        
        let mut child = Command::new(&self.cli_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn decompiler: {}", e))?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())
                .map_err(|e| anyhow!("Failed to write to decompiler stdin: {}", e))?;
        }
        
        let output = child.wait_with_output()
            .map_err(|e| anyhow!("Failed to wait for decompiler: {}", e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        parse_decompiler_response(&stdout)
    }
}

/// Persistent decompiler server (reuses single process for multiple requests)
pub struct DecompilerServer {
    cli_path: std::path::PathBuf,
    sla_dir: String,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout_reader: Option<BufReader<ChildStdout>>,
}

impl DecompilerServer {
    /// Create new decompiler server (process not started yet)
    pub fn new<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str) -> Result<Self> {
        let path = cli_path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(anyhow!("Decompiler CLI not found: {:?}", path));
        }
        Ok(Self {
            cli_path: path,
            sla_dir: sla_dir.to_string(),
            child: None,
            stdin: None,
            stdout_reader: None,
        })
    }

    /// Ensure server process is running
    fn ensure_started(&mut self) -> Result<()> {
        if self.child.is_some() {
            // Check if process is still alive
            if let Some(ref mut child) = self.child {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        // Process exited, need to restart
                        self.child = None;
                        self.stdin = None;
                        self.stdout_reader = None;
                    }
                    Ok(None) => return Ok(()), // Still running
                    Err(_) => {
                        self.child = None;
                        self.stdin = None;
                        self.stdout_reader = None;
                    }
                }
            }
        }

        // Start new server process
        // Note: stderr is inherited to avoid buffer blocking, logs go to console
        let mut child = Command::new(&self.cli_path)
            .arg("--server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn decompiler server: {}", e))?;

        self.stdin = child.stdin.take();
        if let Some(stdout) = child.stdout.take() {
            self.stdout_reader = Some(BufReader::new(stdout));
        }
        self.child = Some(child);

        Ok(())
    }

    /// Decompile bytes using the persistent server
    pub fn decompile(&mut self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Result<String> {
        self.ensure_started()?;

        let bytes_b64 = BASE64.encode(bytes);
        
        let request = format!(
            r#"{{"bytes":"{}","address":{},"is_64bit":{},"sla_dir":"{}"}}"#,
            bytes_b64,
            base_addr,
            if is_64bit { "true" } else { "false" },
            self.sla_dir
        );

        // Send request
        if let Some(ref mut stdin) = self.stdin {
            writeln!(stdin, "{}", request)
                .map_err(|e| anyhow!("Failed to write to server: {}", e))?;
            stdin.flush()
                .map_err(|e| anyhow!("Failed to flush to server: {}", e))?;
        } else {
            return Err(anyhow!("Server stdin not available"));
        }

        // Read response
        if let Some(ref mut reader) = self.stdout_reader {
            let mut response = String::new();
            reader.read_line(&mut response)
                .map_err(|e| anyhow!("Failed to read from server: {}", e))?;
            
            parse_decompiler_response(&response)
        } else {
            Err(anyhow!("Server stdout not available"))
        }
    }

    /// Shutdown the server process
    pub fn shutdown(&mut self) {
        if let Some(ref mut stdin) = self.stdin {
            let _ = writeln!(stdin, r#"{{"cmd":"quit"}}"#);
            let _ = stdin.flush();
        }
        if let Some(ref mut child) = self.child {
            let _ = child.wait();
        }
        self.child = None;
        self.stdin = None;
        self.stdout_reader = None;
    }

    /// Check if server is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }
}

impl Drop for DecompilerServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Thread-safe wrapper for DecompilerServer
pub type SharedDecompilerServer = Arc<Mutex<DecompilerServer>>;

/// Create a shared decompiler server
pub fn create_shared_server<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str) -> Result<SharedDecompilerServer> {
    let server = DecompilerServer::new(cli_path, sla_dir)?;
    Ok(Arc::new(Mutex::new(server)))
}

/// Parse decompiler JSON response and extract code
fn parse_decompiler_response(response: &str) -> Result<String> {
    // Try to extract result
    if response.contains("\"status\":\"ok\"") {
        if let Some(start) = response.find("\"code\":\"") {
            let start = start + 8;
            let chars: Vec<char> = response.chars().collect();
            let mut end = start;
            while end < chars.len() {
                if chars[end] == '"' && (end == start || chars[end - 1] != '\\') {
                    break;
                }
                end += 1;
            }
            let code = &response[start..end];
            let code = code
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
            return Ok(code);
        }
    }
    
    // Check for error status
    if response.contains("\"status\":\"error\"") {
        if let Some(start) = response.find("\"message\":\"") {
            let start = start + 11;
            if let Some(end) = response[start..].find("\"") {
                let msg = &response[start..start + end];
                return Err(anyhow!("Decompiler error: {}", msg));
            }
        }
        return Err(anyhow!("Decompiler error: {}", response));
    }
    
    Err(anyhow!("Invalid response format: {}", response))
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
