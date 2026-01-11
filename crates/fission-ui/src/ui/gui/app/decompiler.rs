//! Decompiler operations - Function decompilation using native FFI.

use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
// use std::time::Instant;

use super::decomp_worker::DecompileRequest;
use crate::analysis::disasm::DisasmEngine;
use crate::core::config::CONFIG;
// use crate::ui::gui::core::domain::CachedDecompile;
use crate::ui::gui::core::state::AppState;
use fission_loader::loader::FunctionInfo;

/// Decompile a function (sends request to worker thread)
pub fn decompile_function(
    state: &mut AppState,
    decomp_tx: &Sender<DecompileRequest>,
    latest_request_id: &Arc<AtomicU64>,
    func: &FunctionInfo,
) {
    // Skip import functions
    if func.is_import {
        state.log(format!(
            "[!] {} is an import function (no code to decompile)",
            func.name
        ));
        state.analysis.domain.decompiled_code = format!(
            "// {} is an imported function\n// Address: 0x{:x}\n// No code available - this is a stub pointing to external library",
            func.name, func.address
        );
        return;
    }

    // Persistant caching is now handled at the analysis engine level
    // in CachingDecompiler. We no longer need to check cache here.

    // CRITICAL: Check if binary is loaded AND decompiler context is ready
    if state.analysis.domain.loaded_binary.as_ref().is_none() {
        state.log("[!] No binary loaded".to_string());
        state.analysis.domain.decompiled_code =
            "// No binary loaded\n// Use File → Open to load a binary".to_string();
        return;
    }

    // Extra safety: Check decompiler context is loaded (prevents race conditions)
    if !state.analysis.domain.decompiler_context_loaded {
        state.log("[!] Decompiler context not ready".to_string());
        state.analysis.domain.decompiled_code =
            "// Decompiler initializing...\n// Please wait a moment and try again".to_string();
        return;
    }

    let address = func.address;
    let (bytes, is_64bit, binary_hash) = {
        let binary = match state.analysis.domain.loaded_binary.as_ref() {
            Some(b) => b,
            None => {
                state.analysis.domain.decompiled_code = "// Error: No binary loaded".to_string();
                return;
            }
        };

        // Get function bytes (use config default if size is unknown)
        let mut func_size = if func.size > 0 {
            func.size as usize
        } else {
            CONFIG.decompiler.default_function_size
        };

        // Limit function size to not exceed section bounds
        for section in &binary.sections {
            if section.is_executable
                && address >= section.virtual_address
                && address < section.virtual_address + section.virtual_size
            {
                let max_size = (section.virtual_address + section.virtual_size - address) as usize;
                func_size = func_size.min(max_size);
                break;
            }
        }

        // Clamp to configured min/max sizes
        func_size = func_size
            .max(CONFIG.decompiler.min_function_size)
            .min(CONFIG.decompiler.max_function_size);

        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        (bytes, binary.is_64bit, binary.hash.clone())
    };

    // Disassemble bytes (synchronous, fast)
    match DisasmEngine::new(is_64bit) {
        Ok(engine) => match engine.disassemble(&bytes, address) {
            Ok(insns) => {
                state.analysis.domain.asm_instructions = insns;
            }
            Err(e) => {
                state.log(format!("[!] Disassembly error: {}", e));
                state.analysis.domain.asm_instructions.clear();
            }
        },
        Err(e) => {
            state.log(format!("[!] Failed to initialize disassembler: {}", e));
            state.analysis.domain.asm_instructions.clear();
        }
    }

    // Guard: Prevent re-requesting decompilation of the same address if already in progress
    if state.analysis.domain.decompiling {
        // Check if user is clicking the same function that's currently being decompiled
        if let Some(ref selected) = state.analysis.domain.selected_function {
            if selected.address == address {
                // Already decompiling this function, skip request
                return;
            }
        }
    }

    state.analysis.domain.decompiling = true;
    state.analysis.domain.decompiled_code = format!("// Decompiling 0x{:x}...", address);
    state.log(format!(
        "[*] Decompiling 0x{:x} ({} bytes)",
        address,
        bytes.len()
    ));

    // Generate new request ID (for debouncing)
    let request_id = latest_request_id.fetch_add(1, Ordering::SeqCst) + 1;
    latest_request_id.store(request_id, Ordering::SeqCst);

    // Send request to worker thread (non-blocking)
    // Send request to worker thread (non-blocking)
    // Optimization: If decompiler context is loaded, send empty bytes to use persistent memory
    let request_bytes = if state.analysis.domain.decompiler_context_loaded {
        Vec::new()
    } else {
        bytes.to_vec()
    };

    let request = DecompileRequest {
        request_id,
        binary_id: binary_hash.clone(),
        bytes: request_bytes,
        address,
        is_64bit,
        is_prefetch: false,
        is_binary_load: false,
        image_base: 0,
        iat_symbols: std::collections::HashMap::new(),
        global_symbols: std::collections::HashMap::new(),
        functions: Vec::new(),
        gdt_json_path: None,
        sections: Vec::new(),
        binary_hash,
        is_cfg_request: false,
        is_clear_cache: false,
    };

    if let Err(e) = decomp_tx.send(request) {
        state.log(format!("[!] Failed to send decompile request: {}", e));
        state.analysis.domain.decompiling = false;
    }
}

/// Tokenize decompiled code for faster rendering and highlight support
fn tokenize_decompiled_code(code: &str) -> Vec<Vec<crate::ui::gui::core::viewmodels::DecomToken>> {
    use crate::ui::gui::core::viewmodels::DecomToken;
    use crate::ui::gui::theme::{catppuccin, code as code_colors};
    use regex::Regex;
    use std::sync::LazyLock;

    static C_KEYWORDS: &[&str] = &[
        "if", "else", "while", "for", "return", "break", "continue", "switch", "case", "default",
        "do", "goto", "sizeof", "asm", "__asm", "volatile", "restrict", "inline", "extern",
        "static", "const", "register", "auto", "volatile",
    ];

    static C_TYPES: &[&str] = &[
        "void",
        "int",
        "char",
        "short",
        "long",
        "unsigned",
        "signed",
        "float",
        "double",
        "struct",
        "union",
        "enum",
        "typedef",
        "uint8_t",
        "uint16_t",
        "uint32_t",
        "uint64_t",
        "int8_t",
        "int16_t",
        "int32_t",
        "int64_t",
        "size_t",
        "bool",
        "long long",
        "long double",
        "uint128_t",
        "int128_t",
        "byte",
        "word",
        "dword",
        "qword",
        "undefined",
        "undefined1",
        "undefined2",
        "undefined4",
        "undefined8",
    ];

    // Regex for various C tokens:
    static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?x)
            (?P<comment>//.*|/\*.*?\*/) |
            (?P<string>h?"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])') |
            (?P<ident>[a-zA-Z_][a-zA-Z0-9_]*) |
            (?P<number>0x[0-9a-fA-F]+|[0-9]+) |
            (?P<op>[!%^&*()\-+=\[\]{}|;:,.<>?/])
        "#,
        )
        .unwrap()
    });

    code.lines()
        .map(|line| {
            let mut line_tokens = Vec::new();
            let mut last_pos = 0;

            for cap in TOKEN_RE.captures_iter(line) {
                let m = cap.get(0).unwrap();

                // Add whitespace/skipped text as a non-clickable token
                if m.start() > last_pos {
                    line_tokens.push(DecomToken {
                        text: line[last_pos..m.start()].to_string(),
                        color: catppuccin::TEXT,
                        is_clickable: false,
                        is_function_call: false,
                    });
                }

                let text = m.as_str();
                let mut color = catppuccin::TEXT;
                let mut is_clickable = false;
                let mut is_function_call = false;

                if cap.name("comment").is_some() {
                    color = code_colors::COMMENT;
                } else if cap.name("string").is_some() {
                    color = code_colors::STRING;
                } else if let Some(_ident) = cap.name("ident") {
                    is_clickable = true;
                    if C_KEYWORDS.contains(&text) {
                        color = code_colors::KEYWORD;
                        is_clickable = false;
                    } else if C_TYPES.contains(&text) {
                        color = code_colors::TYPE;
                        is_clickable = false;
                    } else {
                        // Check if it's a function call (followed by '(')
                        let remaining = &line[m.end()..];
                        if remaining.trim_start().starts_with('(') {
                            color = code_colors::FUNCTION;
                            is_function_call = true;
                        } else {
                            color = catppuccin::TEXT;
                        }
                    }
                } else if cap.name("number").is_some() {
                    color = code_colors::NUMBER;
                    // Check if it's a hex address (0x prefix with 6+ hex digits)
                    if text.starts_with("0x") && text.len() >= 8 {
                        is_clickable = true;
                        is_function_call = true; // Treat as potential function address
                    }
                } else if cap.name("op").is_some() {
                    color = code_colors::OPERATOR;
                }

                line_tokens.push(DecomToken {
                    text: text.to_string(),
                    color,
                    is_clickable,
                    is_function_call,
                });

                last_pos = m.end();
            }

            // Remaining trailing text
            if last_pos < line.len() {
                line_tokens.push(DecomToken {
                    text: line[last_pos..].to_string(),
                    color: catppuccin::TEXT,
                    is_clickable: false,
                    is_function_call: false,
                });
            }

            line_tokens
        })
        .collect()
}

/// Store decompile result in UI state
pub fn cache_decompile_result(state: &mut AppState, address: u64, c_code: String) {
    // Collect all symbols (IAT + User defined)
    let mut symbols = if let Some(ref binary) = state.analysis.domain.loaded_binary {
        binary.iat_symbols.clone()
    } else {
        std::collections::HashMap::new()
    };

    // Add user-defined function names (overwrites IAT if conflict)
    for (addr, name) in &state.analysis.domain.user_function_names {
        symbols.insert(*addr, name.clone());
    }

    let mut processed_code = apply_symbols(&c_code, &symbols);

    // Get display name for the header
    let func_name = symbols
        .get(&address)
        .cloned()
        .unwrap_or_else(|| format!("sub_{:x}", address));

    // Create a nice header
    let header = format!(
        "/*\n * Function: {}\n * Address:  0x{:x}\n */\n\n",
        func_name, address
    );

    // Inject user comment if present for this address
    if let Some(comment) = state.analysis.domain.user_comments.get(&address) {
        processed_code = format!("// Comment: {}\n{}", comment, processed_code);
    }

    let final_code = format!("{}{}", header, processed_code);

    state.analysis.domain.decompiled_code = final_code.clone();
    state.analysis.domain.decompiling = false;

    // Prefill tokenized cache for fast rendering
    state.viewmodels.decompile.tokenized_lines = tokenize_decompiled_code(&final_code);
}

/// Replace pcRamXXXXXXXX, func_0xXXXXXXXX, DAT_XXXXXXXX, and LAB_XXXXXXXX patterns
/// with actual symbol names (IAT or user-defined)
fn apply_symbols(code: &str, symbols: &std::collections::HashMap<u64, String>) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    if symbols.is_empty() {
        return code.to_string();
    }

    // Pattern for Ghidra/Decompiler artifacts:
    // pcRam, func_0x, DAT_, LAB_, ptr_, case_0x, switch_0x
    // Followed by 8 or 16 hex digits
    static COMBINED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:pcRam|func_|DAT_|LAB_|ptr_|case_|switch_|0x)([0-9a-f]{8,16})").unwrap()
    });

    // Single pass replacement
    let result = COMBINED_RE.replace_all(code, |caps: &regex::Captures| {
        let addr_str = &caps[1];
        if let Ok(addr) = u64::from_str_radix(addr_str, 16) {
            if let Some(name) = symbols.get(&addr) {
                return name.clone();
            }
        }
        caps[0].to_string()
    });

    result.into_owned()
}
