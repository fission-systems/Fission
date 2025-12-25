//! Integration tests for CLI command parsing
//!
//! Tests the CLI command parsing and validation logic.

use fission::ui::cli::commands::{Command, parse_command, parse_address};

/// Test basic command parsing - help
#[test]
fn test_parse_help_command() {
    assert!(matches!(parse_command("help"), Command::Help));
    assert!(matches!(parse_command("h"), Command::Help));
    assert!(matches!(parse_command("?"), Command::Help));
}

/// Test quit/exit commands
#[test]
fn test_parse_quit_commands() {
    assert!(matches!(parse_command("quit"), Command::Quit));
    assert!(matches!(parse_command("exit"), Command::Quit));
    assert!(matches!(parse_command("q"), Command::Quit));
}

/// Test load command parsing
#[test]
fn test_parse_load_command() {
    match parse_command("load /path/to/file.exe") {
        Command::Load(path) => assert_eq!(path, "/path/to/file.exe"),
        _ => panic!("Expected Load command"),
    }
    
    // Load without path should be Unknown
    assert!(matches!(parse_command("load"), Command::Unknown(_)));
}

/// Test functions command
#[test]
fn test_parse_functions_command() {
    assert!(matches!(parse_command("functions"), Command::Functions));
    assert!(matches!(parse_command("funcs"), Command::Functions));
    assert!(matches!(parse_command("f"), Command::Functions));
}

/// Test sections command
#[test]
fn test_parse_sections_command() {
    assert!(matches!(parse_command("sections"), Command::Sections));
    assert!(matches!(parse_command("sec"), Command::Sections));
}

/// Test strings command
#[test]
fn test_parse_strings_command() {
    assert!(matches!(parse_command("strings"), Command::Strings));
    assert!(matches!(parse_command("str"), Command::Strings));
}

/// Test disassemble command with address
#[test]
fn test_parse_disassemble_command() {
    match parse_command("disasm 0x401000") {
        Command::Disasm { addr, count: _ } => assert_eq!(addr, 0x401000),
        _ => panic!("Expected Disasm command"),
    }
    
    // Without address should be Unknown
    assert!(matches!(parse_command("disasm"), Command::Unknown(_)));
}

/// Test decompile command with address
#[test]
fn test_parse_decompile_command() {
    match parse_command("decompile 0x140001000") {
        Command::Decompile(addr) => assert_eq!(addr, 0x140001000),
        _ => panic!("Expected Decompile command"),
    }
    
    // Short form
    match parse_command("dec 0x1000") {
        Command::Decompile(addr) => assert_eq!(addr, 0x1000),
        _ => panic!("Expected Decompile command"),
    }
}

/// Test info command
#[test]
fn test_parse_info_command() {
    assert!(matches!(parse_command("info"), Command::Info));
    assert!(matches!(parse_command("i"), Command::Info));
}

/// Test clear command
#[test]
fn test_parse_clear_command() {
    assert!(matches!(parse_command("clear"), Command::Clear));
    assert!(matches!(parse_command("cls"), Command::Clear));
}

/// Test empty and whitespace handling
#[test]
fn test_parse_empty_input() {
    assert!(matches!(parse_command(""), Command::Unknown(_)));
    assert!(matches!(parse_command("   "), Command::Unknown(_)));
}

/// Test unknown command handling
#[test]
fn test_parse_unknown_command() {
    match parse_command("foobar xyz 123") {
        Command::Unknown(cmd) => assert_eq!(cmd, "foobar"), // Only stores the command name
        _ => panic!("Expected Unknown command"),
    }
}

/// Test address parsing with various formats
#[test]
fn test_parse_address_formats() {
    // Hex format with 0x prefix
    assert_eq!(parse_address("0x1234"), Some(0x1234));
    assert_eq!(parse_address("0X1234"), Some(0x1234));
    
    // Hex format with hex digits
    assert_eq!(parse_address("0xDEADBEEF"), Some(0xDEADBEEF));
    
    // Decimal format
    assert_eq!(parse_address("1000"), Some(1000));
    
    // Invalid format
    assert_eq!(parse_address("not_a_number"), None);
    assert_eq!(parse_address(""), None);
}

/// Test case insensitivity
#[test]
fn test_command_case_insensitivity() {
    assert!(matches!(parse_command("HELP"), Command::Help));
    assert!(matches!(parse_command("Help"), Command::Help));
    assert!(matches!(parse_command("QUIT"), Command::Quit));
}

/// Test analyze command
#[test]
fn test_parse_analyze_command() {
    assert!(matches!(parse_command("analyze"), Command::Analyze));
    assert!(matches!(parse_command("anal"), Command::Analyze));
    assert!(matches!(parse_command("a"), Command::Analyze));
}
