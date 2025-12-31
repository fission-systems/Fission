/**
 * Fission Post-Processors
 * 
 * Post-processing functions for decompiled C code:
 * - IAT symbol replacement
 * - String literal inlining
 * - Function signature application
 * - Context-aware constant substitution
 * - Fallback constant replacement
 */

#ifndef FISSION_POST_PROCESSORS_H
#define FISSION_POST_PROCESSORS_H

#include <string>
#include <map>
#include <vector>
#include <sstream>
#include <iomanip>
#include <cstdint>
#include <cstring>
#include <cctype>
#include <algorithm>

#include "constants.h"

// ============================================================================
// IAT Symbol Replacement
// ============================================================================

inline std::string post_process_iat_calls(const std::string& code, const std::map<uint64_t, std::string>& iat_symbols) {
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

inline std::string inline_strings(const std::string& code, const std::map<uint64_t, std::string>& string_table) {
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
        
        // Escape string for C literal
        std::string escaped;
        for (char c : str) {
            if (c == '"') escaped += "\\\"";
            else if (c == '\\') escaped += "\\\\";
            else if (c == '\n') escaped += "\\n";
            else if (c == '\r') escaped += "\\r";
            else if (c == '\t') escaped += "\\t";
            else if (c >= 0x20 && c <= 0x7E) escaped += c;
            else {
                char hex[8];
                snprintf(hex, sizeof(hex), "\\x%02x", (unsigned char)c);
                escaped += hex;
            }
        }
        
        if (escaped.length() > 60) {
            escaped = escaped.substr(0, 57) + "...";
        }
        
        std::string replacement = "\"" + escaped + "\"";
        
        for (const auto& pat : patterns) {
            size_t pos = 0;
            while ((pos = result.find(pat, pos)) != std::string::npos) {
                size_t end = pos + pat.length();
                if (end < result.length() && std::isxdigit(result[end])) {
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

inline std::string apply_function_signatures(const std::string& code) {
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

inline std::string smart_constant_replace(const std::string& code) {
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

inline std::string post_process_constants(const std::string& code, const std::map<uint64_t, std::string>& enum_values) {
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

#endif // FISSION_POST_PROCESSORS_H
