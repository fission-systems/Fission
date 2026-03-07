#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"

#include <string>
#include <map>
#include <vector>
#include <algorithm>
#include <cctype>
#include <cstdio>
#include <cstring>

namespace fission {
namespace processing {

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
                
                // Try to detect bitwise OR expression: 0xA | 0xB | 0xC
                // Combine all hex literals in the arg connected by '|'.
                auto group_it = ENUM_GROUPS.find(mapping.enum_group);
                if (group_it != ENUM_GROUPS.end()) {
                    // Count how many 0x literals appear
                    size_t first_hex = arg.find("0x");
                    size_t second_hex = (first_hex != std::string::npos)
                        ? arg.find("0x", first_hex + 2) : std::string::npos;
                    
                    if (first_hex != std::string::npos && second_hex != std::string::npos) {
                        // Multiple hex literals — parse all and OR them together
                        uint64_t combined = 0;
                        size_t scan = first_hex;
                        size_t span_start = first_hex;
                        size_t span_end = first_hex;
                        bool valid = true;
                        
                        while (scan != std::string::npos && scan < arg.length()) {
                            if (arg.substr(scan, 2) != "0x") { valid = false; break; }
                            size_t hex_end = scan + 2;
                            while (hex_end < arg.length() && std::isxdigit(arg[hex_end])) hex_end++;
                            if (hex_end == scan + 2) { valid = false; break; }
                            
                            std::string hex_str = arg.substr(scan, hex_end - scan);
                            combined |= std::stoull(hex_str, nullptr, 16);
                            span_end = hex_end;
                            
                            // Skip whitespace and '|'
                            size_t next = hex_end;
                            while (next < arg.length() && (arg[next] == ' ' || arg[next] == '\t')) next++;
                            if (next < arg.length() && arg[next] == '|') {
                                next++;
                                while (next < arg.length() && (arg[next] == ' ' || arg[next] == '\t')) next++;
                                scan = next;
                            } else {
                                break;
                            }
                        }
                        
                        if (valid) {
                            std::string resolved = resolve_flag_combination(combined, group_it->second);
                            if (!resolved.empty()) {
                                arg.replace(span_start, span_end - span_start, resolved);
                            }
                        }
                    } else if (first_hex != std::string::npos) {
                        // Single hex literal (original path)
                        size_t hex_end = first_hex + 2;
                        while (hex_end < arg.length() && std::isxdigit(arg[hex_end])) hex_end++;
                        
                        std::string hex_str = arg.substr(first_hex, hex_end - first_hex);
                        uint64_t value = std::stoull(hex_str, nullptr, 16);
                        
                        std::string resolved = resolve_flag_combination(value, group_it->second);
                        if (!resolved.empty()) {
                            arg.replace(first_hex, hex_end - first_hex, resolved);
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

std::string substitute_guids(const std::string& code, const std::map<std::string, std::string>& guid_map) {
    if (guid_map.empty() || code.empty()) return code;
    
    // Scan code for GUID-shaped patterns (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)
    // and look them up in the map. This is O(N) in code size instead of O(M*N)
    // where M = number of GUIDs in the map (typically thousands).
    std::string result;
    result.reserve(code.size());
    
    size_t i = 0;
    while (i < code.size()) {
        // Quick check: need at least 36 chars remaining for a GUID
        if (i + 36 <= code.size() && std::isxdigit(static_cast<unsigned char>(code[i]))) {
            // Check 8-4-4-4-12 pattern: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
            bool looks_like_guid = true;
            // Check dashes at positions 8, 13, 18, 23
            if (code[i+8] != '-' || code[i+13] != '-' || code[i+18] != '-' || code[i+23] != '-') {
                looks_like_guid = false;
            }
            
            if (looks_like_guid) {
                // Verify all hex digit positions
                static const int hex_positions[] = {
                    0,1,2,3,4,5,6,7,  // 8 hex
                    9,10,11,12,        // 4 hex
                    14,15,16,17,       // 4 hex
                    19,20,21,22,       // 4 hex
                    24,25,26,27,28,29,30,31,32,33,34,35  // 12 hex
                };
                bool valid = true;
                for (int p : hex_positions) {
                    if (!std::isxdigit(static_cast<unsigned char>(code[i+p]))) {
                        valid = false;
                        break;
                    }
                }
                
                if (valid) {
                    std::string candidate = code.substr(i, 36);
                    auto it = guid_map.find(candidate);
                    if (it != guid_map.end()) {
                        result.append(it->second);
                        i += 36;
                        continue;
                    }
                    // Also try uppercase
                    std::string upper = candidate;
                    for (auto& c : upper) c = toupper(c);
                    it = guid_map.find(upper);
                    if (it != guid_map.end()) {
                        result.append(it->second);
                        i += 36;
                        continue;
                    }
                }
            }
        }
        result.push_back(code[i]);
        i++;
    }
    return result;
}

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

} // namespace processing
} // namespace fission
