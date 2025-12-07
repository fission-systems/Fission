//! Integration Tests for gRPC Decompiler Client
//! 
//! Run with: cargo test --features native_decomp --test client_test -- --nocapture

#[cfg(test)]
mod tests {
    use fission::decomp::client::GhidraClient;

    #[tokio::test]
    async fn test_grpc_connection() {
        println!("Attempting to connect to Ghidra Server...");
        
        match GhidraClient::connect().await {
            Ok(mut client) => {
                println!("✅ Connected to Ghidra Server!");
                
                // Test Ping
                match client.ping().await {
                   Ok(alive) => println!("   Ping: {}", alive),
                   Err(e) => println!("   Ping failed: {}", e),
                }

                // Test Load
                let dummy = vec![0x90; 100];
                let _ = client.load_binary(dummy, 0x1000, "x86:LE:64:default").await;

                // Test Bulk Decompile
                match client.decompile_function(0x1000).await {
                    Ok(resp) => {
                        println!("   Decompile success!");
                        println!("   Signature: {}", resp.signature);
                        println!("   Blocks: {}", resp.blocks.len());
                        for block in resp.blocks {
                            println!("     Block {:x}-{:x} ({} instrs)", 
                                block.start_addr, block.end_addr, block.instructions.len());
                        }
                    }
                    Err(e) => println!("   Decompile error (expected if no server): {}", e),
                }
            }
            Err(e) => {
                println!("⚠️ Connection failed (Expected if ghidra_server.exe missing): {}", e);
            }
        }
    }
}
