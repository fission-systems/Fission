//! String cross-reference analysis command

use crate::ui::cli::handlers::CliState;
use colored::Colorize;
use fission_analysis::analysis::string_xrefs;

pub fn cmd_string_xrefs(state: &CliState, search_term: &str, min_length: usize) {
    match &state.binary {
        Some(binary) => {
            println!();
            println!("{}", "String Cross-Reference Analysis".cyan().bold());
            println!("{} {}", "Search Term:".dimmed(), search_term.cyan());
            println!(
                "{} {}",
                "Min Length:".dimmed(),
                min_length.to_string().cyan()
            );
            println!();

            // Analyze string xrefs
            println!("{}", "Analyzing strings and cross-references...".dimmed());
            let analysis = string_xrefs::analyze_string_xrefs(binary, min_length);

            // Show statistics
            let stats = analysis.stats();
            println!("{}", "Statistics:".yellow());
            println!(
                "  Total Strings:       {}",
                stats.total_strings.to_string().cyan()
            );
            println!(
                "  Referenced:          {} ({}%)",
                stats.referenced_strings.to_string().green(),
                if stats.total_strings > 0 {
                    (stats.referenced_strings * 100 / stats.total_strings).to_string()
                } else {
                    "0".to_string()
                }
            );
            println!(
                "  Unreferenced:        {}",
                stats.unreferenced_strings.to_string().dimmed()
            );
            println!(
                "  ASCII:               {}",
                stats.ascii_strings.to_string().cyan()
            );
            println!(
                "  Unicode:             {}",
                stats.unicode_strings.to_string().cyan()
            );
            println!(
                "  Total Cross-Refs:    {}",
                stats.total_xrefs.to_string().green()
            );
            println!();

            // Search for matching strings
            let results = if search_term.starts_with('/') && search_term.ends_with('/') {
                // Regex search
                let pattern = &search_term[1..search_term.len() - 1];
                match analysis.find_by_regex(pattern) {
                    Ok(r) => r,
                    Err(e) => {
                        println!("{} Invalid regex: {}", "[!]".red(), e);
                        return;
                    }
                }
            } else if search_term.starts_with('"') && search_term.ends_with('"') {
                // Exact match
                let exact = &search_term[1..search_term.len() - 1];
                analysis.find_by_content(exact)
            } else {
                // Partial match (contains)
                analysis.find_by_partial(search_term)
            };

            if results.is_empty() {
                println!("{}", "No matching strings found".dimmed());
                return;
            }

            println!(
                "{} {} {}",
                "Found".yellow(),
                results.len().to_string().cyan(),
                "matching string(s):".yellow()
            );
            println!();

            // Display results
            for (i, result) in results.iter().enumerate() {
                if i >= 50 {
                    println!(
                        "  {}",
                        format!("... and {} more (showing first 50)", results.len() - 50).dimmed()
                    );
                    break;
                }

                let string = &result.string;
                let type_str = match string.string_type {
                    fission_analysis::analysis::strings::StringType::Ascii => "ASCII".green(),
                    fission_analysis::analysis::strings::StringType::Unicode => "UTF16".blue(),
                };

                // Truncate long strings
                let display_content = if string.content.len() > 60 {
                    format!("{}...", &string.content[..60])
                } else {
                    string.content.clone()
                };

                println!(
                    "{} {} @ 0x{:08x} ({})",
                    "▸".cyan(),
                    display_content.yellow(),
                    string.address,
                    type_str
                );

                // Show cross-references
                if result.xrefs.is_empty() {
                    println!("    {}", "No references".dimmed());
                } else {
                    println!(
                        "    {} {}",
                        "References:".dimmed(),
                        format!("({} found)", result.xrefs.len()).dimmed()
                    );

                    for (j, xref) in result.xrefs.iter().enumerate() {
                        if j >= 10 {
                            println!(
                                "      {}",
                                format!("... and {} more", result.xrefs.len() - 10).dimmed()
                            );
                            break;
                        }

                        let type_str = match xref.xref_type {
                            fission_analysis::analysis::xrefs::XrefType::Call => "CALL".green(),
                            fission_analysis::analysis::xrefs::XrefType::Jump => "JUMP".yellow(),
                            fission_analysis::analysis::xrefs::XrefType::Data => "DATA".blue(),
                        };

                        // Find function name for the caller
                        let caller_name = binary
                            .functions
                            .iter()
                            .find(|f| {
                                xref.from_addr >= f.address && xref.from_addr < f.address + f.size
                            })
                            .map(|f| f.name.as_str())
                            .unwrap_or("unknown");

                        println!(
                            "      {} 0x{:08x}  {}",
                            type_str,
                            xref.from_addr,
                            caller_name.dimmed()
                        );
                    }
                }

                println!();
            }
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}
