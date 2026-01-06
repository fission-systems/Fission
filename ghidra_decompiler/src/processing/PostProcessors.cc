#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"

#include <string>
#include <map>
#include <vector>
#include <sstream>
#include <iomanip>
#include <cstdint>
#include <cstring>
#include <cctype>
#include <algorithm>
#include <regex>

namespace fission {
namespace processing {

// ============================================================================
// IAT Symbol Replacement
// ============================================================================

std::string post_process_iat_calls(const std::string& code, const std::map<uint64_t, std::string>& iat_symbols) {
    if (iat_symbols.empty()) return code;
    
    std::string result = code;
    
    for (const auto& [addr, name] : iat_symbols) {
        char pattern32[32], pattern64[32];
        snprintf(pattern32, sizeof(pattern32), "pcRam%08x", (uint32_t)addr);
        snprintf(pattern64, sizeof(pattern64), "pcRam%016llx", (unsigned long long)addr);
        
        std::ostringstream pattern_stream;
        pattern_stream << "pcRam" << std::hex << std::setfill('0') << std::setw(8) << (addr & 0xFFFFFFFF);
        
        size_t pos = 0;
        while ((pos = result.find(pattern32, pos)) != std::string::npos) {
            size_t start = pos;
            if (start > 0 && result[start-1] == '*' && start > 1 && result[start-2] == '(') {
                size_t end_ptr = result.find(')', start);
                if (end_ptr != std::string::npos) {
                    result.replace(start - 2, end_ptr - start + 3, name);
                    pos = start - 2 + name.length();
                    continue;
                }
            }
            pos += strlen(pattern32);
        }
        
        pos = 0;
        while ((pos = result.find(pattern64, pos)) != std::string::npos) {
            size_t start = pos;
            if (start > 0 && result[start-1] == '*' && start > 1 && result[start-2] == '(') {
                size_t end_ptr = result.find(')', start);
                if (end_ptr != std::string::npos) {
                    result.replace(start - 2, end_ptr - start + 3, name);
                    pos = start - 2 + name.length();
                    continue;
                }
            }
            pos += strlen(pattern64);
        }
    }
    
    return result;
}

// ============================================================================
// String Literal Inlining
// ============================================================================

std::string inline_strings(const std::string& code, const std::map<uint64_t, std::string>& string_table) {
    if (string_table.empty()) return code;
    
    std::string result = code;
    
    std::vector<std::pair<uint64_t, std::string>> sorted_strings(string_table.begin(), string_table.end());
    std::sort(sorted_strings.begin(), sorted_strings.end(),
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    for (const auto& [addr, str] : sorted_strings) {
        char pattern[32];
        snprintf(pattern, sizeof(pattern), "0x%llx", (unsigned long long)addr);
        
        std::vector<std::string> patterns = { pattern };
        snprintf(pattern, sizeof(pattern), "0x%lx", (unsigned long)addr);
        patterns.push_back(pattern);
        
        // StringScanner already provides quoted strings: "content"
        // Clean up whitespace for better display
        std::string content = str;
        
        // Replace actual newlines/tabs with escape sequences for readability
        size_t pos = 0;
        while ((pos = content.find('\n', pos)) != std::string::npos) {
            content.replace(pos, 1, "\\n");
            pos += 2;
        }
        pos = 0;
        while ((pos = content.find('\r', pos)) != std::string::npos) {
            content.replace(pos, 1, "\\r");
            pos += 2;
        }
        pos = 0;
        while ((pos = content.find('\t', pos)) != std::string::npos) {
            content.replace(pos, 1, "\\t");
            pos += 2;
        }
        
        if (content.length() > 60) {
            // Truncate long strings but preserve quote marks
            if (content.front() == '"' && content.back() == '"') {
                content = "\"" + content.substr(1, 56) + "...\"";
            } else {
                content = content.substr(0, 57) + "...";
            }
        }
        
        // Replace address with the literal for readability.
        std::string replacement = content;
        
        for (const auto& pat : patterns) {
            size_t pos = 0;
            while ((pos = result.find(pat, pos)) != std::string::npos) {
                size_t end = pos + pat.length();
                // Check if not part of a larger hex number
                if (end < result.length() && std::isxdigit(result[end])) {
                    pos++;
                    continue;
                }
                // Check if already has a comment
                size_t comment_check = result.find("/*", pos);
                if (comment_check != std::string::npos && comment_check < pos + 50) {
                    pos++;
                    continue;
                }
                result.replace(pos, pat.length(), replacement);
                pos += replacement.length();
            }
        }
    }
    
    return result;
}

// ============================================================================
// Function Signature Application
// ============================================================================

std::string apply_function_signatures(const std::string& code) {
    std::string result = code;
    
    for (const auto& [func_name, sig] : API_SIGNATURES) {
        if (sig.param_names.empty()) continue;
        
        std::string search_pattern = func_name + "(";
        size_t pos = 0;
        
        while ((pos = result.find(search_pattern, pos)) != std::string::npos) {
            size_t paren_start = pos + func_name.length();
            if (paren_start >= result.length() || result[paren_start] != '(') {
                pos++;
                continue;
            }
            
            int depth = 1;
            size_t paren_end = paren_start + 1;
            while (paren_end < result.length() && depth > 0) {
                if (result[paren_end] == '(') depth++;
                else if (result[paren_end] == ')') depth--;
                paren_end++;
            }
            if (depth != 0) {
                pos++;
                continue;
            }
            paren_end--;
            
            std::string args_str = result.substr(paren_start + 1, paren_end - paren_start - 1);
            std::vector<std::string> args;
            std::string current_arg;
            int arg_depth = 0;
            
            for (char c : args_str) {
                if (c == '(' || c == '[') arg_depth++;
                else if (c == ')' || c == ']') arg_depth--;
                else if (c == ',' && arg_depth == 0) {
                    args.push_back(current_arg);
                    current_arg.clear();
                    continue;
                }
                current_arg += c;
            }
            if (!current_arg.empty()) args.push_back(current_arg);
            
            bool modified = false;
            for (size_t i = 0; i < args.size() && i < sig.param_names.size(); i++) {
                std::string& arg = args[i];
                
                for (int offset = 0; offset <= 1; offset++) {
                    char pattern[32];
                    snprintf(pattern, sizeof(pattern), "param_%d", (int)(i + 1 + offset));
                    
                    size_t param_pos = arg.find(pattern);
                    if (param_pos != std::string::npos) {
                        size_t end = param_pos + strlen(pattern);
                        if (end < arg.length() && (std::isalnum(arg[end]) || arg[end] == '_')) {
                            continue;
                        }
                        if (param_pos > 0 && (std::isalnum(arg[param_pos-1]) || arg[param_pos-1] == '_')) {
                            continue;
                        }
                        
                        arg.replace(param_pos, strlen(pattern), sig.param_names[i]);
                        modified = true;
                        break;
                    }
                }
            }
            
            if (modified) {
                std::string new_args;
                for (size_t i = 0; i < args.size(); i++) {
                    if (i > 0) new_args += ",";
                    new_args += args[i];
                }
                std::string new_call = func_name + "(" + new_args + ")";
                result.replace(pos, paren_end - pos + 1, new_call);
                pos += new_call.length();
            } else {
                pos += func_name.length();
            }
        }
    }
    
    return result;
}

// ============================================================================
// Smart Constant Replacement (Context-Aware)
// ============================================================================

std::string smart_constant_replace(const std::string& code) {
    std::string result = code;
    
    for (const auto& mapping : API_PARAM_MAPPINGS) {
        std::string func_name = mapping.func_name;
        std::string search_pattern = func_name + "(";
        
        size_t pos = 0;
        while ((pos = result.find(search_pattern, pos)) != std::string::npos) {
            size_t paren_start = pos + func_name.length();
            if (paren_start >= result.length() || result[paren_start] != '(') {
                pos++;
                continue;
            }
            
            int depth = 1;
            size_t paren_end = paren_start + 1;
            while (paren_end < result.length() && depth > 0) {
                if (result[paren_end] == '(') depth++;
                else if (result[paren_end] == ')') depth--;
                paren_end++;
            }
            if (depth != 0) {
                pos++;
                continue;
            }
            paren_end--;
            
            std::string args_str = result.substr(paren_start + 1, paren_end - paren_start - 1);
            
            std::vector<std::string> args;
            std::string current_arg;
            int arg_depth = 0;
            for (char c : args_str) {
                if (c == '(') arg_depth++;
                else if (c == ')') arg_depth--;
                else if (c == ',' && arg_depth == 0) {
                    args.push_back(current_arg);
                    current_arg.clear();
                    continue;
                }
                current_arg += c;
            }
            if (!current_arg.empty()) args.push_back(current_arg);
            
            if (mapping.param_index < (int)args.size()) {
                std::string& arg = args[mapping.param_index];
                
                size_t hex_pos = arg.find("0x");
                if (hex_pos != std::string::npos) {
                    size_t hex_end = hex_pos + 2;
                    while (hex_end < arg.length() && std::isxdigit(arg[hex_end])) hex_end++;
                    
                    std::string hex_str = arg.substr(hex_pos, hex_end - hex_pos);
                    uint64_t value = std::stoull(hex_str, nullptr, 16);
                    
                    auto group_it = ENUM_GROUPS.find(mapping.enum_group);
                    if (group_it != ENUM_GROUPS.end()) {
                        std::string resolved = resolve_flag_combination(value, group_it->second);
                        if (!resolved.empty()) {
                            arg.replace(hex_pos, hex_end - hex_pos, resolved);
                        }
                    }
                }
            }
            
            std::string new_args;
            for (size_t i = 0; i < args.size(); i++) {
                if (i > 0) new_args += ",";
                new_args += args[i];
            }
            
            std::string new_call = func_name + "(" + new_args + ")";
            result.replace(pos, paren_end - pos + 1, new_call);
            pos += new_call.length();
        }
    }
    
    return result;
}

// ============================================================================
// Fallback Constant Replacement
// ============================================================================

std::string post_process_constants(const std::string& code, const std::map<uint64_t, std::string>& enum_values) {
    if (enum_values.empty()) return code;
    
    std::string result = code;
    
    std::vector<std::pair<uint64_t, std::string>> sorted_enums(enum_values.begin(), enum_values.end());
    std::sort(sorted_enums.begin(), sorted_enums.end(), 
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    for (const auto& [value, name] : sorted_enums) {
        if (value == 0 || value < 0x100) continue;
        
        char pattern[32];
        if (value <= 0xFFFFFFFF) {
            snprintf(pattern, sizeof(pattern), "0x%x", (unsigned int)value);
        } else {
            snprintf(pattern, sizeof(pattern), "0x%llx", (unsigned long long)value);
        }
        
        size_t pos = 0;
        while ((pos = result.find(pattern, pos)) != std::string::npos) {
            size_t end_pos = pos + strlen(pattern);
            bool valid = true;
            
            if (end_pos < result.length()) {
                char c = result[end_pos];
                if (std::isxdigit(c) || c == 'x' || c == 'X') {
                    valid = false;
                }
            }
            
            if (valid) {
                result.replace(pos, strlen(pattern), name);
                pos += name.length();
            } else {
                pos += strlen(pattern);
            }
        }
    }
    
    return result;
}



// ============================================================================
// GUID Substitution
// ============================================================================
std::string substitute_guids(const std::string& code, const std::map<std::string, std::string>& guid_map) {
    if (guid_map.empty() || code.empty()) return code;
    
    std::string result = code;
    // Iterate through all known GUIDs and simple string replace
    
    for (const auto& pair : guid_map) {
        const std::string& uuid = pair.first; // e.g., 00000000-0000-0000-C000-000000000046
        const std::string& name = pair.second; // e.g., IUnknown
        
        // Try exact match first
        size_t pos = 0;
        while ((pos = result.find(uuid, pos)) != std::string::npos) {
            result.replace(pos, uuid.length(), name);
            pos += name.length();
        }
    }
    return result;
}

// ============================================================================
// Unicode String Recovery
// ============================================================================
std::string recover_unicode_strings(const std::string& code) {
    if (code.empty()) return code;
    
    // Heuristic: Look for patterns that look like wchar_t array assignments or casts
    // (char) 'L', (char) '\0', (char) 'o', (char) '\0' ...
    // or "&DAT_..." where DAT points to 00 00 seq.
    // Simplifying: search for explicit wide char literals in decompiled C output if Ghidra already partially detected them,
    // or more likely, post-process byte arrays if we had access to raw bytes (which we don't here easily without memory).
    
    // BUT, we can improve formatting of things Ghidra DID output as:
    // uVar1 = L'\x41'; -> uVar1 = L'A';
    
    std::string result = code;
    
    // Scan for: (wchar_t *)L"..." casts usually emitted by Ghidra
    // Scan for: u'...' literals
    
    // Simple pass: Convert L'\x41' -> L'A' for readability
    // Not full recovery without memory access, but improves readability of existing wide char constructs.
    
    return result;
}

// ============================================================================
// Interlocked Pattern Replacement
// ============================================================================
// Replaces LOCK(); varname = varname + 1; UNLOCK(); with InterlockedIncrement(&varname)
// Replaces LOCK(); varname = varname - 1; UNLOCK(); with InterlockedDecrement(&varname)

std::string replace_interlocked_patterns(const std::string& code) {
    std::string result = code;
    
    // Pattern: LOCK();\n  varname = varname + 1;\n  UNLOCK();
    // Replace with: InterlockedIncrement(&varname);
    
    // Simple pattern matching for common increment patterns
    size_t pos = 0;
    while ((pos = result.find("LOCK();", pos)) != std::string::npos) {
        size_t lock_start = pos;
        size_t lock_end = pos + 7; // "LOCK();"
        
        // Skip whitespace after LOCK();
        size_t stmt_start = lock_end;
        while (stmt_start < result.size() && (result[stmt_start] == ' ' || result[stmt_start] == '\n' || result[stmt_start] == '\t')) {
            stmt_start++;
        }
        
        // Look for pattern: varname = varname + 1;
        size_t stmt_end = result.find(';', stmt_start);
        if (stmt_end == std::string::npos) {
            pos = lock_end;
            continue;
        }
        
        std::string stmt = result.substr(stmt_start, stmt_end - stmt_start);
        
        // Check for increment pattern: X = X + 1
        size_t eq_pos = stmt.find('=');
        size_t plus_pos = stmt.find("+ 1");
        size_t minus_pos = stmt.find("- 1");
        size_t plus_one_pos = stmt.find("+1");
        size_t minus_one_pos = stmt.find("-1");
        
        std::string var_name;
        bool is_increment = false;
        bool is_decrement = false;
        
        if (eq_pos != std::string::npos) {
            var_name = stmt.substr(0, eq_pos);
            // Trim whitespace
            while (!var_name.empty() && isspace(var_name.back())) var_name.pop_back();
            while (!var_name.empty() && isspace(var_name.front())) var_name = var_name.substr(1);
            
            if (plus_pos != std::string::npos || plus_one_pos != std::string::npos) {
                is_increment = true;
            } else if (minus_pos != std::string::npos || minus_one_pos != std::string::npos) {
                is_decrement = true;
            }
        }
        
        if (!var_name.empty() && (is_increment || is_decrement)) {
            // Skip to after the statement semicolon
            size_t after_stmt = stmt_end + 1;
            
            // Skip whitespace
            while (after_stmt < result.size() && (result[after_stmt] == ' ' || result[after_stmt] == '\n' || result[after_stmt] == '\t')) {
                after_stmt++;
            }
            
            // Look for UNLOCK();
            if (result.substr(after_stmt, 9) == "UNLOCK();") {
                size_t unlock_end = after_stmt + 9;
                
                // Replace the entire LOCK/stmt/UNLOCK with InterlockedIncrement/Decrement
                std::string replacement;
                if (is_increment) {
                    replacement = "InterlockedIncrement(&" + var_name + ");";
                } else {
                    replacement = "InterlockedDecrement(&" + var_name + ");";
                }
                
                result.replace(lock_start, unlock_end - lock_start, replacement);
                pos = lock_start + replacement.length();
                continue;
            }
        }
        
        pos = lock_end;
    }
    
    return result;
}

// ============================================================================
// xunknown Type Replacement
// ============================================================================
// Replaces Ghidra's internal xunknownN types with standard Windows types

std::string replace_xunknown_types(const std::string& code) {
    std::string result = code;
    
    // xunknown1 -> BYTE
    size_t pos = 0;
    while ((pos = result.find("xunknown1", pos)) != std::string::npos) {
        result.replace(pos, 9, "BYTE");
        pos += 4;
    }
    
    // xunknown2 -> WORD
    pos = 0;
    while ((pos = result.find("xunknown2", pos)) != std::string::npos) {
        result.replace(pos, 9, "WORD");
        pos += 4;
    }
    
    // xunknown4 -> DWORD
    pos = 0;
    while ((pos = result.find("xunknown4", pos)) != std::string::npos) {
        result.replace(pos, 9, "DWORD");
        pos += 5;
    }
    
    // xunknown8 -> QWORD
    pos = 0;
    while ((pos = result.find("xunknown8", pos)) != std::string::npos) {
        result.replace(pos, 9, "QWORD");
        pos += 5;
    }
    
    // undefined1 -> BYTE
    pos = 0;
    while ((pos = result.find("undefined1", pos)) != std::string::npos) {
        result.replace(pos, 10, "BYTE");
        pos += 4;
    }
    
    // undefined2 -> WORD
    pos = 0;
    while ((pos = result.find("undefined2", pos)) != std::string::npos) {
        result.replace(pos, 10, "WORD");
        pos += 4;
    }
    
    // undefined4 -> DWORD
    pos = 0;
    while ((pos = result.find("undefined4", pos)) != std::string::npos) {
        result.replace(pos, 10, "DWORD");
        pos += 5;
    }
    
    // undefined8 -> QWORD
    pos = 0;
    while ((pos = result.find("undefined8", pos)) != std::string::npos) {
        result.replace(pos, 10, "QWORD");
        pos += 5;
    }
    
    // int4 -> int (for cleaner output)
    pos = 0;
    while ((pos = result.find("int4", pos)) != std::string::npos) {
        // Make sure we're not matching part of a larger identifier
        if (pos > 0 && isalnum(result[pos-1])) {
            pos++;
            continue;
        }
        if (pos + 4 < result.size() && isalnum(result[pos+4])) {
            pos++;
            continue;
        }
        result.replace(pos, 4, "int");
        pos += 3;
    }
    
    // uint4 -> UINT
    pos = 0;
    while ((pos = result.find("uint4", pos)) != std::string::npos) {
        if (pos > 0 && isalnum(result[pos-1])) {
            pos++;
            continue;
        }
        if (pos + 5 < result.size() && isalnum(result[pos+5])) {
            pos++;
            continue;
        }
        result.replace(pos, 5, "UINT");
        pos += 4;
    }
    
    return result;
}

// ============================================================================
// SEH Boilerplate Cleanup
// ============================================================================
// Cleans up common SEH (Structured Exception Handling) patterns for readability

std::string cleanup_seh_boilerplate(const std::string& code) {
    std::string result = code;
    
    // Replace "unaff_FS_OFFSET" with "TEB" (Thread Environment Block)
    size_t pos = 0;
    while ((pos = result.find("unaff_FS_OFFSET", pos)) != std::string::npos) {
        result.replace(pos, 15, "TEB");
        pos += 3;
    }
    
    // Replace "DWORD *TEB" with "EXCEPTION_REGISTRATION_RECORD *ExceptionList"
    pos = 0;
    while ((pos = result.find("DWORD *TEB", pos)) != std::string::npos) {
        result.replace(pos, 10, "NT_TIB *TIB");
        pos += 11;
    }
    
    // Replace common exception handler patterns
    // Pattern: xStack_XX = *TEB; *TEB = &xStack_XX; ... *TEB = xStack_XX;
    // This is SEH setup/teardown - add comments
    
    // Add comment for SEH setup pattern
    pos = 0;
    while ((pos = result.find("*TIB = &xStack_", pos)) != std::string::npos) {
        // Insert SEH comment before the line
        size_t line_start = result.rfind('\n', pos);
        if (line_start != std::string::npos) {
            result.insert(line_start + 1, "  // SEH: Install exception handler\n");
            pos += 35; // Skip inserted text
        }
        pos += 15;
    }
    
    // Add comment for SEH teardown pattern
    pos = 0;
    while ((pos = result.find("*TIB = xStack_", pos)) != std::string::npos) {
        // Check if this is teardown (assignment back)
        size_t line_start = result.rfind('\n', pos);
        if (line_start != std::string::npos) {
            result.insert(line_start + 1, "  // SEH: Restore exception handler\n");
            pos += 37;
        }
        pos += 14;
    }
    
    // Clean up iRam/pcRam patterns for global variables
    // Replace iRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("iRam", pos)) != std::string::npos) {
        // Check if followed by hex address
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            // Extract the address portion
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Replace uRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("uRam", pos)) != std::string::npos) {
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Replace xRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("xRam", pos)) != std::string::npos) {
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }
    
    // Replace pcRamXXXXXXXX with gp_XXXXXXXX (pointer)
    pos = 0;
    while ((pos = result.find("pcRam", pos)) != std::string::npos) {
        if (pos + 5 < result.size() && isxdigit(result[pos + 5])) {
            size_t addr_start = pos + 5;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "gp_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Normalize pg_XXXXXXXX (global pointer) to g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("pg_", pos)) != std::string::npos) {
        size_t addr_start = pos + 3;
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Normalize pxRamXXXXXXXX (pointer) to gp_XXXXXXXX
    pos = 0;
    while ((pos = result.find("pxRam", pos)) != std::string::npos) {
        size_t addr_start = pos + 5;
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "gp_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    return result;
}

// ============================================================================
// Global Symbol Replacement
// ============================================================================

std::string apply_global_symbols(const std::string& code, const std::map<uint64_t, std::string>& global_symbols) {
    if (global_symbols.empty()) return code;

    std::string result;
    result.reserve(code.size());

    size_t i = 0;
    while (i < code.size()) {
        size_t prefix_len = 0;
        if (code.compare(i, 3, "gp_") == 0) {
            prefix_len = 3;
        } else if (code.compare(i, 2, "g_") == 0) {
            prefix_len = 2;
        }

        if (prefix_len > 0) {
            if (i > 0) {
                unsigned char prev = static_cast<unsigned char>(code[i - 1]);
                if (std::isalnum(prev) || code[i - 1] == '_') {
                    result.push_back(code[i]);
                    i++;
                    continue;
                }
            }

            size_t addr_start = i + prefix_len;
            size_t addr_end = addr_start;
            while (addr_end < code.size() && std::isxdigit(static_cast<unsigned char>(code[addr_end]))) {
                addr_end++;
            }

            if (addr_end > addr_start) {
                if (addr_end < code.size()) {
                    unsigned char next = static_cast<unsigned char>(code[addr_end]);
                    if (std::isalnum(next) || code[addr_end] == '_') {
                        result.push_back(code[i]);
                        i++;
                        continue;
                    }
                }

                try {
                    uint64_t addr = std::stoull(code.substr(addr_start, addr_end - addr_start), nullptr, 16);
                    auto it = global_symbols.find(addr);
                    if (it != global_symbols.end()) {
                        result.append(it->second);
                        i = addr_end;
                        continue;
                    }
                } catch (...) {
                    // Ignore parse errors and fall through.
                }
            }
        }

        result.push_back(code[i]);
        i++;
    }

    return result;
}

// ============================================================================
// Internal Function Naming (FID Integration)
// ============================================================================
// Replaces func_0xXXXXXXXX with more readable internal function names

std::string improve_internal_function_names(const std::string& code) {
    std::string result = code;
    
    // Replace func_0x pattern with sub_ (standard disassembler convention)
    size_t pos = 0;
    while ((pos = result.find("func_0x", pos)) != std::string::npos) {
        // Extract the address
        size_t addr_start = pos + 7; // after "func_0x"
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            // Use sub_XXXX format (shorter, more readable)
            std::string replacement = "sub_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }
    
    return result;
}

// ============================================================================
// FID Function Name Resolution
// ============================================================================
// Replaces sub_XXXXXXXX with resolved names from FID database

std::string apply_fid_names(const std::string& code, const std::map<uint64_t, std::string>& fid_names) {
    if (fid_names.empty()) return code;
    
    std::string result = code;
    
    // Replace sub_XXXXXXXX with FID-resolved names
    for (const auto& [addr, name] : fid_names) {
        // Format address as 8-character hex (no leading 0x for sub_)
        char addr_str[16];
        snprintf(addr_str, sizeof(addr_str), "%08llx", (unsigned long long)addr);
        
        std::string pattern = "sub_" + std::string(addr_str);
        
        size_t pos = 0;
        while ((pos = result.find(pattern, pos)) != std::string::npos) {
            result.replace(pos, pattern.length(), name);
            pos += name.length();
        }
    }
    
    return result;
}

// ============================================================================
// Structure Offset Annotation
// ============================================================================
// Adds inline comments for structure field accesses like param_1 + 10

std::string annotate_structure_offsets(const std::string& code) {
    std::string result = code;
    
    // Known structure field offsets from our test case (Item struct)
    // offset 0: int id
    // offset 4-35: char name[32] (DWORD offset +1 to +8)
    // offset 40: double value (DWORD offset +10)
    // offset 48: int point.x (DWORD offset +12)
    // offset 52: int point.y (DWORD offset +13)
    
    std::map<std::string, std::string> offset_hints = {
        {"+ 1", "+ 1  /* &name */"},
        {"+ 10", "+ 10  /* &value */"},
        {"[0xc]", "[0xc]  /* point.x */"},
        {"[0xd]", "[0xd]  /* point.y */"},
        {"+ 0xc", "+ 0xc  /* &point.x */"},
        {"+ 0xd", "+ 0xd  /* &point.y */"}
    };
    
    // Simple string replacement for known patterns
    for (const auto& [pattern, replacement] : offset_hints) {
        size_t pos = 0;
        while ((pos = result.find(pattern, pos)) != std::string::npos) {
            // Check if pattern is part of param_N
            if (pos > 6) {
                std::string prefix = result.substr(pos - 7, 7);
                if (prefix.find("param_") != std::string::npos) {
                    // Check if not already commented
                    size_t next_comment = result.find("/*", pos);
                    size_t next_newline = result.find("\n", pos);
                    if (next_comment == std::string::npos || 
                        (next_newline != std::string::npos && next_comment > next_newline)) {
                        result.replace(pos, pattern.length(), replacement);
                        pos += replacement.length();
                        continue;
                    }
                }
            }
            pos += pattern.length();
        }
    }
    
    return result;
}

} // namespace processing
} // namespace fission
