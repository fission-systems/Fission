//! Command processing
//!
//! Shared command definitions used by both CLI and GUI.

/// Application commands that can be executed
#[derive(Debug, Clone)]
pub enum AppCommand {
    /// Load a binary file
    LoadBinary(String),
    /// List functions
    ListFunctions,
    /// Decompile a function at address
    Decompile(u64),
    /// Start debugging
    StartDebug,
    /// Stop debugging
    StopDebug,
    /// Show help
    Help,
    /// Clear console/logs
    Clear,
    /// Quit application
    Quit,
    /// Unknown command
    Unknown(String),
}

/// Parse a command string into AppCommand
pub fn parse_command(input: &str) -> AppCommand {
    let input = input.trim();
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts.first().unwrap_or(&"");
    let arg = parts.get(1).map(|s| s.trim());

    match *cmd {
        "load" | "open" | "o" => {
            if let Some(path) = arg {
                AppCommand::LoadBinary(path.to_string())
            } else {
                AppCommand::Unknown("load requires a path".into())
            }
        }
        "funcs" | "functions" => AppCommand::ListFunctions,
        "decompile" | "dec" => {
            if let Some(addr_str) = arg {
                if let Some(addr) = parse_address(addr_str) {
                    AppCommand::Decompile(addr)
                } else {
                    AppCommand::Unknown("Invalid address".into())
                }
            } else {
                AppCommand::Unknown("decompile requires an address".into())
            }
        }
        "start" => AppCommand::StartDebug,
        "stop" => AppCommand::StopDebug,
        "help" | "?" => AppCommand::Help,
        "clear" => AppCommand::Clear,
        "quit" | "exit" | "q" => AppCommand::Quit,
        _ => AppCommand::Unknown(input.to_string()),
    }
}

/// Parse an address from hex or decimal string
fn parse_address(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() > 4 {
        u64::from_str_radix(s, 16).ok()
    } else {
        s.parse().ok()
    }
}
