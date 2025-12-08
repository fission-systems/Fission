use super::client::GhidraClient;

#[tokio::test]
async fn test_grpc_connection() {
    println!("Attempting to connect to Ghidra Server...");
    
    // Ensure .sla existence verification logic could be added here if needed

    match GhidraClient::connect().await {
        Ok(mut client) => {
            println!("✅ Connected to Ghidra Server!");
            
            // Test Ping
            match client.ping().await {
               Ok(alive) => println!("   Ping: {}", alive),
               Err(e) => println!("   Ping failed: {}", e),
            }

            // Test Load
            // x86-64 function: int add(int a, int b) { return a + b; }
            // Windows x64: rcx=first param, rdx=second param, return in eax
            // push rbp; mov rbp,rsp; mov eax,ecx; add eax,edx; pop rbp; ret
            let test_func: Vec<u8> = vec![
                0x55,                   // push rbp
                0x48, 0x89, 0xe5,       // mov rbp, rsp
                0x89, 0xc8,             // mov eax, ecx  (a -> eax)
                0x01, 0xd0,             // add eax, edx  (eax += b)
                0x5d,                   // pop rbp
                0xc3,                   // ret
            ];
            if let Err(e) = client.load_binary(test_func, 0x1000, "x86:LE:64:default").await {
                println!("❌ Load Binary failed: {}", e);
            } else {
                println!("✅ Load Binary success");
            }

            // Test Bulk Decompile
            match client.decompile_function(0x1000).await {
                Ok(resp) => {
                    println!("   Decompile success!");
                    println!("   Signature: {}", resp.signature);
                    println!("   Blocks: {}", resp.blocks.len());
                    for block in &resp.blocks {
                        println!("     Block {:x}-{:x} ({} instrs)", 
                            block.start_addr, block.end_addr, block.instructions.len());
                    }
                    println!("\n=== Generated C Code ===");
                    println!("{}", resp.c_code);
                    println!("========================\n");
                }
                Err(e) => println!("   Decompile error: {}", e),
            }
        }
        Err(e) => {
            println!("⚠️ Connection failed (Expected if ghidra_server.exe missing): {}", e);
        }
    }
}
