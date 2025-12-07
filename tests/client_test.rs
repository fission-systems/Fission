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
            }
            Err(e) => {
                println!("⚠️ Connection failed (Expected if ghidra_server.exe missing): {}", e);
            }
        }
    }
}
