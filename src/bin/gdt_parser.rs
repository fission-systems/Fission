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
        eprintln!("Usage: {} <input.gdt> [--dump-types] [--dump-structs] [--dump-fields] [--output <path>]", args[0]);
        std::process::exit(1);
    }
    
    let input_path = &args[1];
    let dump_types = args.iter().any(|a| a == "--dump-types");
    let dump_structs = args.iter().any(|a| a == "--dump-structs");
    let dump_fields = args.iter().any(|a| a == "--dump-fields");
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
    println!("✓ Found {} structure type names", struct_types.len());
    
    // Extract structure definitions
    let structures = GdtParser::extract_structures(&db);
    println!("✓ Parsed {} structure definitions", structures.len());
    
    // Extract field definitions
    let fields = GdtParser::extract_fields(&db);
    println!("✓ Parsed {} field definitions", fields.len());
    
    if dump_types {
        println!("\n=== Structure Type Names ===");
        for (i, name) in struct_types.iter().enumerate() {
            println!("  {:4}. {}", i + 1, name);
        }
    }
    
    if dump_structs {
        println!("\n=== Structure Definitions ===");
        for s in structures.iter().take(50) {
            println!("  {} (size={}, align={}, fields={})", 
                     s.name, s.size, s.alignment, s.field_count);
        }
        if structures.len() > 50 {
            println!("  ... and {} more", structures.len() - 50);
        }
    }
    
    if dump_fields {
        println!("\n=== Field Definitions (sample) ===");
        
        // Group fields by parent_id
        let grouped = GdtParser::extract_fields_grouped(&db);
        let mut parent_ids: Vec<_> = grouped.keys().collect();
        parent_ids.sort();
        
        let mut shown = 0;
        for &parent_id in parent_ids.iter().take(10) {
            if let Some(parent_fields) = grouped.get(&parent_id) {
                println!("\n  [Parent ID 0x{:X}] {} fields:", parent_id, parent_fields.len());
                for f in parent_fields.iter().take(8) {
                    println!("    +{:4}: {} ({} bytes)", f.offset, f.name, f.size);
                }
                if parent_fields.len() > 8 {
                    println!("    ... and {} more fields", parent_fields.len() - 8);
                }
                shown += 1;
            }
        }
        println!("\n  Total: {} parent structures with fields", grouped.len());
    }
    
    // Write DB to file if requested
    if let Some(path) = output_path {
        let mut file = File::create(path).expect("Failed to create output file");
        file.write_all(&db).expect("Failed to write output");
        println!("✓ Wrote DB to: {}", path);
    }
    
    // Generate JSON with full structure info
    let json_path = format!("{}.types.json", input_path);
    
    // Convert structures to JSON-serializable format
    let struct_json: Vec<_> = structures.iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "size": s.size,
            "alignment": s.alignment,
            "field_count": s.field_count,
        })
    }).collect();
    
    // Sample fields (first 1000)
    let field_json: Vec<_> = fields.iter().take(1000).map(|f| {
        serde_json::json!({
            "name": f.name,
            "offset": f.offset,
            "size": f.size,
        })
    }).collect();
    
    let json = serde_json::json!({
        "source": input_path,
        "total_types": all_types.len(),
        "struct_name_count": struct_types.len(),
        "structure_count": structures.len(),
        "field_count": fields.len(),
        "structure_names": struct_types,
        "structures": struct_json,
        "fields_sample": field_json,
    });
    
    let mut json_file = File::create(&json_path).expect("Failed to create JSON file");
    serde_json::to_writer_pretty(&mut json_file, &json).expect("Failed to write JSON");
    println!("✓ Wrote type list to: {}", json_path);
}

