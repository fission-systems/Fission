//! Subprocess-based Ghidra Decompiler interface.
//!
//! Two modes:
//! - `DecompilerServer`: Persistent server process for faster repeated requests
//! - `DecompilerPool`: Pool of server processes for parallel decompilation

use std::process::{Command, Stdio, Child, ChildStdin};
use std::io::{Write, BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

use crate::config::CONFIG;
use crate::core::errors::{Result, FissionError};

/// Maximum consecutive empty lines to tolerate when reading decompiler output.
/// 
/// When the decompiler subprocess sends output, empty lines can occur between
/// chunks. This limit prevents infinite loops if the process becomes unresponsive.
/// After receiving this many consecutive empty lines without valid content,
/// the read operation times out.
/// 
/// This value is tuned for typical decompiler behavior - increase if handling
/// very large functions that may produce sparse output.
const MAX_EMPTY_LINES: usize = 10;

/// Decompiler JSON response structure
#[derive(Deserialize, Debug)]
struct DecompilerResponse {
    status: String,
    code: Option<String>,
    message: Option<String>,
}

/// Persistent decompiler server (reuses single process for multiple requests)
pub struct DecompilerServer {
    cli_path: std::path::PathBuf,
    sla_dir: String,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// Thread for reading stdout asynchronously
    stdout_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// Handle for reader thread (for clean shutdown)
    reader_handle: Option<JoinHandle<()>>,
    /// Context cache for crash recovery
    cached_binary: Option<Vec<u8>>,
    cached_sla_dir: Option<String>,
    cached_image_base: Option<u64>,
    /// Request counter for periodic restart
    request_count: u64,
}

impl DecompilerServer {
    /// Create new decompiler server (process not started yet)
    pub fn new<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str) -> Result<Self> {
        let path = cli_path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(FissionError::decompiler(format!("Decompiler CLI not found: {:?}", path)));
        }
        Ok(Self {
            cli_path: path,
            sla_dir: sla_dir.to_string(),
            child: None,
            stdin: None,
            stdout_rx: None,
            reader_handle: None,
            cached_binary: None,
            cached_sla_dir: None,
            cached_image_base: None,
            request_count: 0,
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
                        self.stdout_rx = None;
                        self.reader_handle = None;
                    }
                    Ok(None) => return Ok(()), // Still running
                    Err(_) => {
                        self.child = None;
                        self.stdin = None;
                        self.stdout_rx = None;
                        self.reader_handle = None;
                    }
                }
            }
        }

        // Start new server process
        // Note: stderr is inherited to avoid buffer blocking AND provide visibility
        let mut child = Command::new(&self.cli_path)
            .arg("--server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| FissionError::decompiler(format!("Failed to spawn decompiler server: {}", e)))?;

        self.stdin = child.stdin.take();
        
        // Spawn background reader thread for stdout
        if let Some(stdout) = child.stdout.take() {
            let (tx, rx) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break; // Receiver dropped
                            }
                        }
                        Err(_) => break, // EOF or error
                    }
                }
            });
            self.stdout_rx = Some(rx);
            self.reader_handle = Some(handle);
        }
        
        self.child = Some(child);

        // Restore context if available (Crash Recovery)
        // Clone data to avoid borrow checker issues with mutable self call
        if let (Some(bytes), Some(sla_dir), Some(image_base)) = (
            self.cached_binary.clone(), 
            self.cached_sla_dir.clone(),
            self.cached_image_base
        ) {
            crate::core::logging::info("[DecompilerServer] Recovering binary context after restart...");
            // Dynamic timeout: base + ~1ms per KB
            let timeout = CONFIG.decompiler.timeout_ms + (bytes.len() as u64 / 1024);
            self.load_binary_internal_with_timeout(&bytes, &sla_dir, image_base, timeout)?;
        }

        Ok(())
    }

    /// Internal helper to send load_bin command with timeout
    fn load_binary_internal_with_timeout(&mut self, bytes: &[u8], sla_dir: &str, image_base: u64, timeout_ms: u64) -> Result<()> {
        let bytes_b64 = BASE64.encode(bytes);
        
        let request = format!(
            r#"{{"load_bin":"{}","sla_dir":"{}","image_base":{}}}"#,
            bytes_b64,
            sla_dir,
            image_base
        );

        if let Some(ref mut stdin) = self.stdin {
            writeln!(stdin, "{}", request)?;
            stdin.flush()?;
        } else {
             return Err(FissionError::decompiler("Server stdin not available"));
        }
        
        // Read response with timeout
        self.read_response_with_timeout(timeout_ms)
            .map(|_| ())
    }

    /// Load complete binary into decompiler memory (Persistent Context)
    pub fn load_binary(&mut self, bytes: &[u8], sla_dir: &str, image_base: u64) -> Result<()> {
        // First ensure process is started (but don't trigger recovery yet)
        self.ensure_started()?;
        
        // Dynamic timeout: base + ~1ms per KB
        let timeout = CONFIG.decompiler.timeout_ms + (bytes.len() as u64 / 1024);
        self.load_binary_internal_with_timeout(bytes, sla_dir, image_base, timeout)
            .map_err(|e| FissionError::decompiler(format!("Failed to load binary: {}", e)))?;
        
        // Cache context for recovery AFTER successful load (not before!)
        self.cached_binary = Some(bytes.to_vec());
        self.cached_sla_dir = Some(sla_dir.to_string());
        self.cached_image_base = Some(image_base);
        
        Ok(())
    }

    /// Decompile bytes using the persistent server
    pub fn decompile(&mut self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Result<String> {
        // Check periodic restart
        let restart_threshold = CONFIG.decompiler.requests_before_restart;
        if restart_threshold > 0 && self.request_count >= restart_threshold {
            crate::core::logging::info(&format!("[DecompilerServer] Restarting after {} requests to reclaim memory", self.request_count));
            self.shutdown();
            self.request_count = 0;
        }

        self.ensure_started()?;
        self.request_count += 1;

        let bytes_b64 = if bytes.is_empty() {
             String::new()
        } else {
             BASE64.encode(bytes)
        };
        
        let request = format!(
            r#"{{"bytes":"{}","address":{},"is_64bit":{},"sla_dir":"{}"}}"#,
            bytes_b64,
            base_addr,
            if is_64bit { "true" } else { "false" },
            self.sla_dir
        );

        if let Some(ref mut stdin) = self.stdin {
            writeln!(stdin, "{}", request)?;
            stdin.flush()?;
        } else {
            return Err(FissionError::decompiler("Server stdin not available"));
        }

        // Read response with timeout
        let response = match self.read_response_with_timeout(CONFIG.decompiler.timeout_ms) {
            Ok(res) => res,
            Err(e) => {
                // Timeout or Error -> Kill process to stop memory leak
                crate::core::logging::warn(&format!("[DecompilerServer] Error/Timeout detected: {}. Restarting process...", e));
                self.shutdown(); // Kill
                return Err(e); // Propagate error, caller can retry
            }
        };
        
        parse_decompiler_response(&response)
    }
    
    /// Helper to read response with timeout (with empty line limit)
    fn read_response_with_timeout(&mut self, timeout_ms: u64) -> Result<String> {
        if let Some(ref rx) = self.stdout_rx {
             match rx.recv_timeout(std::time::Duration::from_millis(timeout_ms)) {
                 Ok(line) => {
                     let mut response = line;
                     let mut empty_retries = 0;
                     
                     // Allow empty lines with retry limit
                     while response.trim().is_empty() {
                          empty_retries += 1;
                          if empty_retries > MAX_EMPTY_LINES {
                              return Err(FissionError::decompiler("Too many empty lines from decompiler"));
                          }
                          match rx.recv_timeout(std::time::Duration::from_millis(timeout_ms)) {
                              Ok(l) => response = l,
                              Err(_) => return Err(FissionError::decompiler("Timeout waiting for non-empty response")),
                          }
                     }
                     Ok(response)
                 },
                 Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                     Err(FissionError::decompiler(format!("Decompiler timed out after {}ms", timeout_ms)))
                 },
                 Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                     Err(FissionError::decompiler("Decompiler process exited unexpectedly"))
                 }
             }
        } else {
             Err(FissionError::decompiler("Stdout receiver not available"))
        }
    }
    
    /// Shutdown the server process
    pub fn shutdown(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill(); // Force kill
            let _ = child.wait();
        }
        self.child = None;
        self.stdin = None;
        self.stdout_rx = None;
        // Join reader thread for clean shutdown (non-blocking wait)
        if let Some(handle) = self.reader_handle.take() {
            // We don't want to block, so just drop it.
            // The thread will exit when stdout closes.
            drop(handle);
        }
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

/// Parse decompiler JSON response using serde_json
fn parse_decompiler_response(response: &str) -> Result<String> {
    let resp: DecompilerResponse = serde_json::from_str(response)
        .map_err(|e| FissionError::decompiler(format!("Invalid JSON response: {} (raw: {})", e, response)))?;
    
    match resp.status.as_str() {
        "ok" => resp.code.ok_or_else(|| FissionError::decompiler("Missing 'code' field in response")),
        "error" => Err(FissionError::decompiler(format!("Decompiler error: {}", resp.message.unwrap_or_default()))),
        _ => Err(FissionError::decompiler(format!("Unknown status: {}", resp.status))),
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

/// Pool of decompiler server processes for parallel decompilation
pub struct DecompilerPool {
    workers: Vec<Mutex<DecompilerServer>>,
    next_worker: std::sync::atomic::AtomicUsize,
}

impl DecompilerPool {
    /// Create a new pool with N worker processes
    pub fn new<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str, num_workers: usize) -> Result<Self> {
        let num_workers = num_workers.max(1); // At least 1 worker
        let mut workers = Vec::with_capacity(num_workers);
        
        for i in 0..num_workers {
            match DecompilerServer::new(&cli_path, sla_dir) {
                Ok(server) => {
                    crate::core::logging::info(&format!("[DecompilerPool] Created worker {}/{}", i + 1, num_workers));
                    workers.push(Mutex::new(server));
                }
                Err(e) => {
                    // If we can't create all workers, use what we have
                    if workers.is_empty() {
                        return Err(e);
                    }
                    crate::core::logging::warn(&format!("[DecompilerPool] Warning: Could not create worker {}: {}", i + 1, e));
                    break;
                }
            }
        }
        
        crate::core::logging::info(&format!("[DecompilerPool] Initialized with {} workers", workers.len()));
        
        Ok(Self {
            workers,
            next_worker: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    
    /// Create pool with default number of workers (CPU cores, max 4)
    pub fn new_default<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str) -> Result<Self> {
        let num_cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(2);
        let num_workers = num_cpus.min(4); // Max 4 workers to avoid memory issues
        Self::new(cli_path, sla_dir, num_workers)
    }
    
    /// Load binary into ALL workers in the pool
    pub fn load_binary(&self, bytes: &[u8], sla_dir: &str, image_base: u64) -> Result<()> {
        let workers_count = self.workers.len();
        let mut success_count = 0;
        
        for (i, worker) in self.workers.iter().enumerate() {
            match worker.lock() {
                Ok(mut guard) => {
                    if let Err(e) = guard.load_binary(bytes, sla_dir, image_base) {
                         crate::core::logging::warn(&format!("[DecompilerPool] Worker {} failed to load binary: {}", i, e));
                    } else {
                         success_count += 1;
                    }
                },
                Err(e) => crate::core::logging::warn(&format!("[DecompilerPool] Failed to lock worker {}: {}", i, e)),
            }
        }
        
        if success_count == 0 && workers_count > 0 {
            Err(FissionError::decompiler("All workers failed to load binary"))
        } else {
            crate::core::logging::info(&format!("[DecompilerPool] Binary loaded into {}/{} workers", success_count, workers_count));
            Ok(())
        }
    }
    
    /// Decompile bytes using next available worker
    /// First tries to find any idle worker (non-blocking), then falls back to round-robin
    pub fn decompile(&self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Result<String> {
        // Strategy 1: Try to find any available worker immediately (non-blocking)
        // This improves load distribution when some workers are processing large functions
        for worker in &self.workers {
            if let Ok(mut guard) = worker.try_lock() {
                return guard.decompile(bytes, base_addr, is_64bit);
            }
        }

        // Strategy 2: All workers busy - use round-robin to queue fairly
        let worker_idx = self.next_worker.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % self.workers.len();
        let mut worker = self.workers[worker_idx].lock()
            .map_err(|_| FissionError::decompiler("Worker mutex poisoned"))?;

        worker.decompile(bytes, base_addr, is_64bit)
    }
    
    /// Try to decompile without blocking (returns None if all workers busy)
    pub fn try_decompile(&self, bytes: &[u8], base_addr: u64, is_64bit: bool) -> Option<Result<String>> {
        // Try each worker, return first one that's available
        for worker in &self.workers {
            if let Ok(mut guard) = worker.try_lock() {
                return Some(guard.decompile(bytes, base_addr, is_64bit));
            }
        }
        None // All workers busy
    }
    
    /// Number of workers in the pool
    pub fn num_workers(&self) -> usize {
        self.workers.len()
    }
    
    /// Shutdown all workers
    pub fn shutdown(&self) {
        for worker in &self.workers {
            if let Ok(mut guard) = worker.lock() {
                guard.shutdown();
            }
        }
    }
}

impl Drop for DecompilerPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Thread-safe shared pool
pub type SharedDecompilerPool = Arc<DecompilerPool>;

/// Create a shared decompiler pool
pub fn create_pool<P: AsRef<std::path::Path>>(cli_path: P, sla_dir: &str, num_workers: usize) -> Result<SharedDecompilerPool> {
    let pool = DecompilerPool::new(cli_path, sla_dir, num_workers)?;
    Ok(Arc::new(pool))
}
