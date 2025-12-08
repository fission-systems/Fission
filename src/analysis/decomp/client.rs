//! gRPC Client for Ghidra Decompiler Service
//!
//! Handles connection to the native C++ server and provides a high-level API.
//! Automatically manages the server process with robust error handling.

use std::fmt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;
use std::time::SystemTime;

pub mod ghidra_service {
    tonic::include_proto!("ghidra_service");
}

use ghidra_service::decompiler_service_client::DecompilerServiceClient;
use ghidra_service::{DecompileRequest, LoadBinaryRequest, PingRequest, FunctionMeta};

// ============================================
// Custom Error Types
// ============================================

/// Errors that can occur during Ghidra client operations
#[derive(Debug)]
pub enum GhidraError {
    /// Server executable not found
    ServerNotFound(String),
    /// Failed to spawn server process
    ServerSpawnFailed(String),
    /// Connection to server timed out
    ConnectionTimeout { attempts: u32, last_error: String },
    /// Server returned an error
    ServerError(String),
    /// Network/transport error during RPC
    TransportError(String),
    /// Binary loading failed
    LoadError(String),
    /// Decompilation failed
    DecompileError(String),
    /// Server process died unexpectedly
    ServerDied,
}

impl fmt::Display for GhidraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GhidraError::ServerNotFound(paths) => {
                write!(f, "Ghidra server not found. Searched: {}", paths)
            }
            GhidraError::ServerSpawnFailed(reason) => {
                write!(f, "Failed to start Ghidra server: {}", reason)
            }
            GhidraError::ConnectionTimeout { attempts, last_error } => {
                write!(f, "Connection timed out after {} attempts. Last error: {}", attempts, last_error)
            }
            GhidraError::ServerError(msg) => {
                write!(f, "Server error: {}", msg)
            }
            GhidraError::TransportError(msg) => {
                write!(f, "Transport error: {}. Try restarting the server.", msg)
            }
            GhidraError::LoadError(msg) => {
                write!(f, "Failed to load binary: {}", msg)
            }
            GhidraError::DecompileError(msg) => {
                write!(f, "Decompilation failed: {}", msg)
            }
            GhidraError::ServerDied => {
                write!(f, "Server process died unexpectedly. Please restart.")
            }
        }
    }
}

impl std::error::Error for GhidraError {}

// For compatibility with anyhow
impl From<tonic::transport::Error> for GhidraError {
    fn from(e: tonic::transport::Error) -> Self {
        GhidraError::TransportError(e.to_string())
    }
}

impl From<tonic::Status> for GhidraError {
    fn from(s: tonic::Status) -> Self {
        GhidraError::ServerError(format!("{}: {}", s.code(), s.message()))
    }
}

pub type Result<T> = std::result::Result<T, GhidraError>;

// ============================================
// Client Configuration
// ============================================

/// Configuration for the Ghidra client
#[derive(Clone)]
pub struct ClientConfig {
    /// Port to connect to
    pub port: u16,
    /// Maximum connection retries
    pub max_retries: u32,
    /// Initial retry delay (doubles each attempt)
    pub initial_retry_delay_ms: u64,
    /// Maximum timeout for decompilation (ms)
    pub decompile_timeout_ms: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            port: 50051,
            max_retries: 5,
            initial_retry_delay_ms: 500,
            decompile_timeout_ms: 30000,
        }
    }
}

// ============================================
// Client Implementation
// ============================================

/// Client wrapper ensuring server connectivity with robust error handling
pub struct GhidraClient {
    client: DecompilerServiceClient<Channel>,
    server_process: Option<Child>,
    config: ClientConfig,
    uri: String,
    current_binary_id: Option<BinaryId>,
    loaded_functions: Vec<FunctionMeta>,
}

impl GhidraClient {
    /// Known paths where the server executable might be located
    const SERVER_PATHS: &'static [&'static str] = &[
        "ghidra_server.exe",                 // PATH
        "build/Release/ghidra_server.exe",   // CMake Release (Windows)
        "build/Debug/ghidra_server.exe",     // CMake Debug (Windows)
        "build/ghidra_server",               // CMake (Unix)
        "./ghidra_server",                   // Current directory
    ];

    /// Connect with default configuration
    pub async fn connect() -> Result<Self> {
        Self::connect_with_config(ClientConfig::default()).await
    }

    /// Connect with custom configuration
    pub async fn connect_with_config(config: ClientConfig) -> Result<Self> {
        let uri = format!("http://[::1]:{}", config.port);
        
        // Try connecting to existing server first
        if let Ok(channel) = Channel::from_shared(uri.clone())
            .map_err(|e| GhidraError::TransportError(e.to_string()))?
            .connect()
            .await 
        {
            log::info!("Connected to existing Ghidra server");
            return Ok(Self {
                client: DecompilerServiceClient::new(channel),
                server_process: None,
                config,
                uri,
                current_binary_id: None,
                loaded_functions: Vec::new(),
            });
        }

        // Start server if not running
        log::info!("Starting Ghidra server...");
        let child = Self::spawn_server()?;
        
        // Wait for server to become ready
        let client = Self::wait_for_server(&uri, &config).await?;
        
        Ok(Self {
            client,
            server_process: Some(child),
            config,
            uri,
            current_binary_id: None,
            loaded_functions: Vec::new(),
        })
    }

    /// Spawn the server process
    fn spawn_server() -> Result<Child> {
        for path in Self::SERVER_PATHS {
            match Command::new(path)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn() 
            {
                Ok(child) => {
                    log::info!("Started server from: {}", path);
                    return Ok(child);
                }
                Err(_) => continue,
            }
        }
        
        Err(GhidraError::ServerNotFound(
            Self::SERVER_PATHS.join(", ")
        ))
    }

    /// Wait for server to become ready with exponential backoff
    async fn wait_for_server(uri: &str, config: &ClientConfig) -> Result<DecompilerServiceClient<Channel>> {
        let mut last_error = String::new();
        
        for attempt in 0..config.max_retries {
            let delay = config.initial_retry_delay_ms * (1 << attempt); // Exponential backoff
            sleep(Duration::from_millis(delay)).await;
            
            match Channel::from_shared(uri.to_string())
                .map_err(|e| GhidraError::TransportError(e.to_string()))?
                .connect()
                .await 
            {
                Ok(channel) => {
                    log::info!("Server ready after {} attempts", attempt + 1);
                    return Ok(DecompilerServiceClient::new(channel));
                }
                Err(e) => {
                    last_error = e.to_string();
                    log::debug!("Attempt {}: {}", attempt + 1, last_error);
                }
            }
        }
        
        Err(GhidraError::ConnectionTimeout {
            attempts: config.max_retries,
            last_error,
        })
    }

    /// Check if server is alive and reconnect if needed
    pub async fn ensure_connected(&mut self) -> Result<()> {
        // Check if server process is still running
        if let Some(ref mut child) = self.server_process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Server has exited
                    self.server_process = None;
                    return Err(GhidraError::ServerDied);
                }
                Ok(None) => {} // Still running
                Err(_) => {}
            }
        }
        
        // Try a ping to verify connection
        match self.ping().await {
            Ok(true) => Ok(()),
            Ok(false) => Err(GhidraError::ServerError("Server not responding".into())),
            Err(GhidraError::TransportError(_)) => {
                // Try to reconnect
                log::warn!("Connection lost, attempting to reconnect...");
                self.client = Self::wait_for_server(&self.uri, &self.config).await?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Load a binary into the server for analysis. Updates current_binary_id and function cache.
    pub async fn load_binary(&mut self, data: Vec<u8>, base_addr: u64, arch: &str, id: BinaryId) -> Result<(bool, &[FunctionMeta])> {
        let request = tonic::Request::new(LoadBinaryRequest {
            binary_content: data,
            base_address: base_addr,
            arch_spec: arch.to_string(),
            sla_path: String::new(),
        });

        let response = self.client.load_binary(request).await?.into_inner();
        
        if response.success {
            self.current_binary_id = Some(id);
            self.loaded_functions = response.functions;
            Ok((true, &self.loaded_functions))
        } else {
            Err(GhidraError::LoadError(response.error_message))
        }
    }

    /// Load only when the binary id has changed
    pub async fn load_binary_if_needed(&mut self, data: Vec<u8>, base_addr: u64, arch: &str, id: BinaryId) -> Result<(bool, &[FunctionMeta])> {
        if let Some(current) = &self.current_binary_id {
            if current == &id {
                return Ok((false, &self.loaded_functions));
            }
        }
        self.load_binary(data, base_addr, arch, id).await
    }

    /// Decompile a function at the given address
    pub async fn decompile_function(&mut self, address: u64) -> Result<ghidra_service::DecompileResponse> {
        let request = tonic::Request::new(DecompileRequest {
            address,
            include_asm: true,
            include_pcode: true,
            timeout_ms: self.config.decompile_timeout_ms,
        });

        let response = self.client.decompile_function(request).await?.into_inner();
        
        if response.success {
            Ok(response)
        } else {
            Err(GhidraError::DecompileError(response.error_message))
        }
    }

    /// Check if server is alive
    pub async fn ping(&mut self) -> Result<bool> {
        let response = self.client.ping(tonic::Request::new(PingRequest {})).await?;
        Ok(response.into_inner().alive)
    }

    /// Get current configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Check if we own the server process
    pub fn owns_server(&self) -> bool {
        self.server_process.is_some()
    }

    /// Snapshot current loaded binary state
    pub fn snapshot_state(&self) -> (Option<BinaryId>, Vec<FunctionMeta>) {
        (self.current_binary_id.clone(), self.loaded_functions.clone())
    }

    /// Restore loaded binary state (used after reconnect)
    pub fn restore_state(&mut self, id: Option<BinaryId>, funcs: Vec<FunctionMeta>) {
        self.current_binary_id = id;
        self.loaded_functions = funcs;
    }
}

impl Drop for GhidraClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.server_process.take() {
            log::info!("Shutting down Ghidra server...");
            let _ = child.kill();
            let _ = child.wait(); // Reap zombie process
        }
    }
}

// ============================================
// Convenience Functions
// ============================================

/// Quick decompile: connect, load, decompile, return C code
pub async fn quick_decompile(
    binary: Vec<u8>, 
    base_addr: u64, 
    func_addr: u64,
    arch: &str
) -> Result<String> {
    let mut client = GhidraClient::connect().await?;
    let id = BinaryId::new(None, binary.len() as u64, arch.to_string(), None);
    client.load_binary(binary, base_addr, arch, id).await?;
    let result = client.decompile_function(func_addr).await?;
    Ok(result.c_code)
}

/// Identifier for a loaded binary to decide when to reload server-side
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryId {
    pub path: Option<String>,
    pub size: u64,
    pub arch: String,
    pub mtime: Option<u64>,
}

impl BinaryId {
    pub fn new(path: Option<String>, size: u64, arch: String, mtime: Option<u64>) -> Self {
        Self { path, size, arch, mtime }
    }
}
