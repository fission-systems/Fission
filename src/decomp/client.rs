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
}

impl GhidraClient {
    const DEFAULT_PORT: u16 = 50051;
    const MAX_RETRIES: u32 = 5;

    pub async fn connect() -> Result<Self> {
        let uri = format!("http://[::1]:{}", Self::DEFAULT_PORT);
        
        if let Ok(channel) = Channel::from_shared(uri.clone())?.connect().await {
            return Ok(Self {
                client: DecompilerServiceClient::new(channel),
                server_process: None,
            });
        }

        log::info!("Starting Ghidra Server...");
        // Try multiple paths to find the server executable
        let paths = [
            "ghidra_server.exe",                 // PATH
            "build/Release/ghidra_server.exe",   // CMake Release (Windows)
            "build/Debug/ghidra_server.exe",     // CMake Debug (Windows)
            "build/ghidra_server",               // CMake (Unix)
        ];
        
        let mut child_res = Err(anyhow!("None tried"));
        for path in paths {
             match Command::new(path)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn() {
                    Ok(c) => { 
                        log::info!("Spawned server from: {}", path);
                        child_res = Ok(c); 
                        break; 
                    }
                    Err(_) => continue,
                }
        }

        let child = child_res.map_err(|_| anyhow!("Failed to spawn ghidra_server: checked {:?}", paths))?;

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
        })
    }

    pub async fn load_binary(&mut self, data: Vec<u8>, base_addr: u64, arch: &str) -> Result<()> {
        let request = tonic::Request::new(LoadBinaryRequest {
            binary_content: data,
            base_address: base_addr,
            arch_spec: arch.to_string(),
            sla_path: "".to_string(),
        });

        let response = self.client.load_binary(request).await?.into_inner();
        
        if response.success {
            Ok(())
        } else {
            Err(anyhow!("Load failed: {}", response.error_message))
        }
    }

    /// Full function analysis
    pub async fn decompile_function(&mut self, address: u64) -> Result<ghidra_service::DecompileResponse> {
        let request = tonic::Request::new(DecompileRequest {
            address,
            include_asm: true,
            include_pcode: true,
            timeout_ms: 30000,
        });

        let response = self.client.decompile_function(request).await?.into_inner();
        
        if response.success {
            Ok(response)
        } else {
            Err(anyhow!("Decompilation failed: {}", response.error_message))
        }
    }

    pub async fn ping(&mut self) -> Result<bool> {
        let response = self.client.ping(tonic::Request::new(PingRequest {})).await?;
        Ok(response.into_inner().alive)
    }
}

impl Drop for GhidraClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.server_process.take() {
            let _ = child.kill();
        }
    }
}
