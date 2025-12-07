//! gRPC Client for Ghidra Decompiler Service
//!
//! Handles connection to the native C++ server and provides a high-level API.
//! Automatically manages the server process.

use anyhow::{anyhow, Result};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;

pub mod ghidra_service {
    tonic::include_proto!("ghidra_service");
}

use ghidra_service::decompiler_service_client::DecompilerServiceClient;
use ghidra_service::{DecompileRequest, DisassembleRequest, LoadBinaryRequest, PingRequest};

/// Client wrapper ensuring server connectivity
pub struct GhidraClient {
    client: DecompilerServiceClient<Channel>,
    server_process: Option<Child>,
    port: u16,
}

impl GhidraClient {
    const DEFAULT_PORT: u16 = 50051;
    const MAX_RETRIES: u32 = 5;

    /// Connect to Ghidra Server (starting it if needed)
    pub async fn connect() -> Result<Self> {
        let uri = format!("http://[::1]:{}", Self::DEFAULT_PORT);
        
        // Try to connect
        if let Ok(channel) = Channel::from_shared(uri.clone())?.connect().await {
            return Ok(Self {
                client: DecompilerServiceClient::new(channel),
                server_process: None, // Already running
                port: Self::DEFAULT_PORT,
            });
        }

        // Not running, spawn server
        log::info!("Starting Ghidra Server...");
        let child = Command::new("ghidra_server.exe") // Assumes in PATH or same dir
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn ghidra_server: {}", e))?;

        // Wait for server to start
        let mut client = None;
        for i in 0..Self::MAX_RETRIES {
            sleep(Duration::from_millis(500 * (i as u64 + 1))).await;
            
            match Channel::from_shared(uri.clone())?.connect().await {
                Ok(channel) => {
                    client = Some(DecompilerServiceClient::new(channel));
                    break;
                }
                Err(_) => continue,
            }
        }

        let client = client.ok_or_else(|| anyhow!("Timed out waiting for server start"))?;
        
        Ok(Self {
            client,
            server_process: Some(child),
            port: Self::DEFAULT_PORT,
        })
    }

    /// Load binary into server
    pub async fn load_binary(&mut self, data: Vec<u8>, base_addr: u64, arch: &str) -> Result<()> {
        let request = tonic::Request::new(LoadBinaryRequest {
            binary_content: data,
            base_address: base_addr,
            arch_spec: arch.to_string(),
            sla_path: "".to_string(), // TODO: Configurable
        });

        let response = self.client.load_binary(request).await?.into_inner();
        
        if response.success {
            Ok(())
        } else {
            Err(anyhow!("Load failed: {}", response.error_message))
        }
    }

    /// Decompile function
    pub async fn decompile(&mut self, address: u64) -> Result<String> {
        let request = tonic::Request::new(DecompileRequest {
            address,
            timeout_ms: 30000,
        });

        let response = self.client.decompile(request).await?.into_inner();
        
        if response.success {
            Ok(response.c_code)
        } else {
            Err(anyhow!("Decompilation failed: {}", response.error_message))
        }
    }

    /// Disassemble instructions
    pub async fn disassemble(&mut self, address: u64, length: u32) -> Result<String> {
        let request = tonic::Request::new(DisassembleRequest {
            address,
            length,
        });

        let response = self.client.disassemble(request).await?.into_inner();
        
        if response.success {
            let mut output = String::new();
            for instr in response.instructions {
                output.push_str(&format!(
                    "{:08x}: {}\t{}\n", 
                    instr.address, instr.mnemonic, instr.operands
                ));
            }
            Ok(output)
        } else {
            Err(anyhow!("Disassembly failed: {}", response.error_message))
        }
    }

    /// Check health
    pub async fn ping(&mut self) -> Result<bool> {
        let response = self.client.ping(tonic::Request::new(PingRequest {})).await?;
        Ok(response.into_inner().alive)
    }
}

impl Drop for GhidraClient {
    fn drop(&mut self) {
        // Kill server if we started it
        if let Some(mut child) = self.server_process.take() {
            let _ = child.kill();
        }
    }
}
