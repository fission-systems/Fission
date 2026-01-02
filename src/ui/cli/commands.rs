//! CLI Command Parsing
//!
//! Parses user input into structured commands.

/// Parsed CLI command
#[derive(Debug, Clone)]
pub enum Command {
    /// Load a binary file
    Load(String),
    /// Show binary info
    Info,
    /// List functions
    Functions,
    /// Disassemble at address with optional count
    Disasm { addr: u64, count: usize },
    /// Decompile function at address
    Decompile(u64),
    /// Extract strings
    Strings,
    /// Show sections
    Sections,
    /// Analyze binary (discover internal functions)
    Analyze,
    /// Show help
    Help,
    /// Clear screen
    Clear,
    /// Quit
    Quit,
    /// Unknown command
    Unknown(String),
}

/// Parse a command string into a Command enum
pub fn parse_command(input: &str) -> Command {
    let input = input.trim();
    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
    let arg1 = parts.get(1).map(|s| s.trim());
    let arg2 = parts.get(2).map(|s| s.trim());

    match cmd.as_str() {
        "load" | "open" | "o" => {
            if let Some(path) = arg1 {
                Command::Load(path.to_string())
            } else {
                Command::Unknown("load requires a path".into())
            }
        }

        "info" | "i" => Command::Info,

        "funcs" | "functions" | "f" => Command::Functions,

        "sections" | "sec" => Command::Sections,

        "strings" | "str" => Command::Strings,

        "analyze" | "anal" | "a" => Command::Analyze,

        "disasm" | "dis" | "d" => {
            if let Some(addr_str) = arg1 {
                if let Some(addr) = parse_address(addr_str) {
                    let count = arg2.and_then(|s| s.parse().ok()).unwrap_or(20);
                    Command::Disasm { addr, count }
                } else {
                    Command::Unknown(format!("Invalid address: {}", addr_str))
                }
            } else {
                Command::Unknown("disasm requires an address".into())
            }
        }

        "decompile" | "dec" | "decomp" => {
            if let Some(addr_str) = arg1 {
                if let Some(addr) = parse_address(addr_str) {
                    Command::Decompile(addr)
                } else {
                    Command::Unknown(format!("Invalid address: {}", addr_str))
                }
            } else {
                Command::Unknown("decompile requires an address".into())
            }
        }

        "help" | "?" | "h" => Command::Help,

        "clear" | "cls" => Command::Clear,

        "quit" | "exit" | "q" => Command::Quit,

        "" => Command::Unknown(String::new()),

        _ => Command::Unknown(cmd),
    }
}

/// Parse an address from hex or decimal string
pub fn parse_address(s: &str) -> Option<u64> {
    let s = s.trim();

    // Handle 0x prefix explicitly
    if let Some(hex_str) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(hex_str, 16).ok();
    }

    // If it looks like hex (long string or contains a-f), parse as hex
    if s.len() > 4 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return u64::from_str_radix(s, 16).ok();
    }

    // Otherwise try decimal first, then hex
    s.parse().ok().or_else(|| u64::from_str_radix(s, 16).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        assert_eq!(parse_address("0x1000"), Some(0x1000));
        assert_eq!(parse_address("0X1000"), Some(0x1000));
        assert_eq!(parse_address("140001000"), Some(0x140001000));
        assert_eq!(parse_address("1000"), Some(1000));
        assert_eq!(parse_address("deadbeef"), Some(0xdeadbeef));
    }

    #[test]
    fn test_parse_command() {
        assert!(matches!(parse_command("load test.exe"), Command::Load(_)));
        assert!(matches!(parse_command("funcs"), Command::Functions));
        assert!(matches!(
            parse_command("disasm 0x1000"),
            Command::Disasm { .. }
        ));
        assert!(matches!(
            parse_command("disasm 0x1000 50"),
            Command::Disasm {
                addr: 0x1000,
                count: 50
            }
        ));
        assert!(matches!(parse_command("quit"), Command::Quit));
        assert!(matches!(parse_command("unknown_cmd"), Command::Unknown(_)));
    }
}
