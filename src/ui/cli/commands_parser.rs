//! Command Parsing
//!
//! Defines CLI commands and parsing logic

/// Available commands in the CLI
#[derive(Debug, Clone)]
pub enum Command {
    Load(String),
    Info,
    Functions,
    Disasm { address: Option<u64>, count: Option<usize> },
    Decompile { address: Option<u64> },
    Strings,
    Sections,
    Analyze,
    Xrefs { address: u64 },
    Help,
    Clear,
    Exit,
    Unknown(String),
}

/// Parse a command string into a Command enum
pub fn parse_command(input: &str) -> Command {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return Command::Unknown("".to_string());
    }

    let cmd = parts[0].to_lowercase();
    match cmd.as_str() {
        "load" => {
            if parts.len() > 1 {
                Command::Load(parts[1..].join(" "))
            } else {
                Command::Unknown("load requires a file path".to_string())
            }
        }
        "info" | "i" => Command::Info,
        "functions" | "funcs" | "f" => Command::Functions,
        "disasm" | "d" => {
            let address = parts.get(1).and_then(|s| parse_addr(s).ok());
            let count = parts.get(2).and_then(|s| s.parse().ok());
            Command::Disasm { address, count }
        }
        "decompile" | "dec" => {
            let address = parts.get(1).and_then(|s| parse_addr(s).ok());
            Command::Decompile { address }
        }
        "strings" | "str" => Command::Strings,
        "sections" | "sects" | "sec" => Command::Sections,
        "analyze" | "anal" | "aa" | "a" => Command::Analyze,
        "xrefs" => {
            if let Some(addr_str) = parts.get(1) {
                if let Ok(addr) = parse_addr(addr_str) {
                    Command::Xrefs { address: addr }
                } else {
                    Command::Unknown("xrefs requires a valid address".to_string())
                }
            } else {
                Command::Unknown("xrefs requires an address".to_string())
            }
        }
        "help" | "h" | "?" => Command::Help,
        "clear" | "cls" => Command::Clear,
        "exit" | "quit" | "q" => Command::Exit,
        _ => Command::Unknown(input.to_string()),
    }
}

fn parse_addr(s: &str) -> Result<u64, std::num::ParseIntError> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse()
    }
}
