//! GDT Parser CLI Tool
//! 
//! Usage: cargo run --bin gdt_parser <path.gdt>

use std::env;
use std::fs::File;
use std::io::Write;

use fission::analysis::gdt_parser::{GdtParser, DbHeader};

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <input.gdt> [--dump-types] [--output <path>]", args[0]);
        std::process::exit(1);
    }
    
    let input_path = &args[1];
    let dump_types = args.iter().any(|a| a == "--dump-types");
    let output_path = args.iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    
    println!("Loading GDT: {}", input_path);
    
    let parser = match GdtParser::load(input_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load GDT: {:?}", e);
            std::process::exit(1);
        }
    };
    
    if !parser.verify_header() {
        eprintln!("Invalid GDT file (missing Java serialization header)");
        std::process::exit(1);
    }
    
    println!("✓ Valid Java serialization header");
    
    let db = match parser.extract_db() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to extract DB: {:?}", e);
            std::process::exit(1);
        }
    };
    
    println!("✓ Extracted DB: {} bytes", db.len());
    
    if let Some(header) = DbHeader::parse(&db) {
        println!("  Signature: {}", header.signature);
        println!("  Version: {}", header.version);
        println!("  Page size: 0x{:X}", header.page_size);
        println!("  Pages: {}", header.page_count);
    }
    
    // Extract type names
    let all_types = GdtParser::extract_type_names(&db);
    println!("✓ Found {} type names", all_types.len());
    
    let struct_types = GdtParser::extract_struct_names(&db);
    println!("✓ Found {} structure types", struct_types.len());
    
    if dump_types {
        println!("\n=== Structure Types ===");
        for (i, name) in struct_types.iter().enumerate() {
            println!("  {:4}. {}", i + 1, name);
        }
    }
    
    // Write DB to file if requested
    if let Some(path) = output_path {
        let mut file = File::create(path).expect("Failed to create output file");
        file.write_all(&db).expect("Failed to write output");
        println!("✓ Wrote DB to: {}", path);
    }
    
    // Generate JSON type list
    let json_path = format!("{}.types.json", input_path);
    let json = serde_json::json!({
        "source": input_path,
        "total_types": all_types.len(),
        "struct_count": struct_types.len(),
        "structures": struct_types,
    });
    
    let mut json_file = File::create(&json_path).expect("Failed to create JSON file");
    serde_json::to_writer_pretty(&mut json_file, &json).expect("Failed to write JSON");
    println!("✓ Wrote type list to: {}", json_path);
}
