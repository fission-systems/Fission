//! GDT Parser CLI Tool
//!
//! Usage: cargo run --bin gdt_parser <path.gdt>

use std::env;
use std::fs::File;
use std::io::Write;

use fission::analysis::gdt_parser::{DbHeader, GdtParser};

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
    let output_path = args
        .iter()
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

    // Extract Data Type ID → Name mapping
    let id_map = GdtParser::extract_datatype_id_map(&db);
    println!("✓ Mapped {} Data Type IDs to names", id_map.len());

    // Extract typedef aliases
    let typedef_map = GdtParser::extract_typedef_map(&db);
    println!("✓ Found {} typedef pointer aliases", typedef_map.len());

    // Build alias resolution map
    let alias_map = GdtParser::build_alias_map(&db);
    println!("✓ Built {} alias → base type mappings", alias_map.len());

    // Extract complete structures with fields
    let complete_structures = GdtParser::extract_complete_structures(&db);
    println!(
        "✓ Resolved {} complete structures with fields",
        complete_structures.len()
    );

    // Extract field definitions (for backward compat)
    let fields = GdtParser::extract_fields(&db);
    println!("✓ Parsed {} field definitions", fields.len());

    if dump_types {
        println!("\n=== Structure Type Names ===");
        for (i, name) in struct_types.iter().enumerate() {
            println!("  {:4}. {}", i + 1, name);
        }
    }

    if dump_structs {
        println!("\n=== Complete Structures (with fields) ===");
        for s in complete_structures.iter().take(30) {
            println!(
                "\n  {} (size={}, align={}, {} fields):",
                s.name, s.size, s.alignment, s.field_count
            );
            for f in s.fields.iter().take(8) {
                println!("    +{:4}: {} ({} bytes)", f.offset, f.name, f.size);
            }
            if s.fields.len() > 8 {
                println!("    ... and {} more", s.fields.len() - 8);
            }
        }
        if complete_structures.len() > 30 {
            println!(
                "\n  ... and {} more structures",
                complete_structures.len() - 30
            );
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
                println!(
                    "\n  [Parent ID 0x{:X}] {} fields:",
                    parent_id,
                    parent_fields.len()
                );
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

    // Convert complete structures to JSON-serializable format (with fields)
    let struct_json: Vec<_> = complete_structures
        .iter()
        .map(|s| {
            let fields_json: Vec<_> = s
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "offset": f.offset,
                        "size": f.size,
                        "ordinal": f.ordinal,
                    })
                })
                .collect();
            serde_json::json!({
                "name": s.name,
                "size": s.size,
                "alignment": s.alignment,
                "field_count": s.field_count,
                "fields": fields_json,
            })
        })
        .collect();

    // ID Map for reference
    let id_map_json: Vec<_> = id_map
        .iter()
        .map(|(id, name)| serde_json::json!({"id": id, "name": name}))
        .collect();

    let json = serde_json::json!({
        "source": input_path,
        "total_types": all_types.len(),
        "struct_name_count": struct_types.len(),
        "complete_structure_count": complete_structures.len(),
        "id_mapping_count": id_map.len(),
        "field_count": fields.len(),
        "typedef_alias_count": alias_map.len(),
        "structure_names": struct_types,
        "complete_structures": struct_json,
        "id_mappings": id_map_json,
        "typedef_aliases": alias_map.iter().take(200).map(|(k, v)| {
            serde_json::json!({"alias": k, "base": v})
        }).collect::<Vec<_>>(),
    });

    let mut json_file = File::create(&json_path).expect("Failed to create JSON file");
    serde_json::to_writer_pretty(&mut json_file, &json).expect("Failed to write JSON");
    println!("✓ Wrote type list to: {}", json_path);
}
