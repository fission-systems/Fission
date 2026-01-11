//! Pcode Graph Generation Command

use colored::Colorize;
use std::path::PathBuf;

use crate::ui::cli::handlers::CliState;

pub fn cmd_graph(state: &CliState, addr: Option<u64>) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
            return;
        }
    };

    let addr = match addr {
        Some(a) => a,
        None => {
            println!(
                "{} Please specify an address: graph <address>",
                "[!]".yellow()
            );
            return;
        }
    };

    #[cfg(feature = "native_decomp")]
    {
        use crate::cli::oneshot::graph::generate_pcode_graph;

        let filename = format!("graph_{:x}.dot", addr);
        let output_path = PathBuf::from(&filename);

        println!(
            "{} Generating Pcode graph for 0x{:x}...",
            "[*]".blue(),
            addr
        );

        match generate_pcode_graph(binary, addr, Some(&output_path), false) {
            Ok(_) => {
                println!("{} Graph saved to {}", "[+]".green(), filename);
                println!(
                    "    Render with: dot -Tpng {} -o {}.png",
                    filename, filename
                );
            }
            Err(e) => {
                println!("{} Failed to generate graph: {}", "[!]".red(), e);
            }
        }
    }

    #[cfg(not(feature = "native_decomp"))]
    {
        println!(
            "{} Graph generation requires 'native_decomp' feature.",
            "[!]".red()
        );
    }
}
