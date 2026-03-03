#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"

#include <string>
#include <map>
#include <regex>
#include <sstream>
#include <iomanip>
#include <cstring>
#include <cctype>

namespace fission {
namespace processing {

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

    // Pass 2: Handle (*_dllname.dll!funcname)(args) pattern.
    // When Ghidra registers an IAT symbol via addFunction(), the C printer
    // outputs the indirect call as (*_api-ms-win-crt-heap-l1-1-0.dll!free)(args).
    // Extract the function name after '!' and preserve the argument list.
    // e.g. (*_ucrtbase.dll!free)(ptr) -> free(ptr)
    {
        static const std::regex iat_indirect_pat(
            R"(\(\*_[A-Za-z0-9._\-]+\.[Dd][Ll][Ll]!([A-Za-z_]\w*)\)(\([^)]*\)))",
            std::regex::optimize
        );
        result = std::regex_replace(result, iat_indirect_pat, "$1$2");
    }

    return result;
}

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

} // namespace processing
} // namespace fission
