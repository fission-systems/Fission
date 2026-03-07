#include "fission/processing/PostProcessors.h"

#include <string>
#include <map>
#include <regex>
#include <sstream>
#include <cctype>
#include <cstdio>
#ifdef _MSC_VER
#include <windows.h>
#include <Dbghelp.h>
#pragma comment(lib, "Dbghelp.lib")
#else
#include <cxxabi.h>
#endif

namespace fission {
namespace processing {

std::string demangle_cpp_names(const std::string& code) {
    if (code.empty()) return code;
    
    std::string result = code;
    
    // 1. Demangle symbols starting with _Z (Itanium ABI) or ? (MSVC)
#ifdef _MSC_VER
    // MSVC: Demangle ?-prefixed symbols using UnDecorateSymbolName
    std::regex mangled_regex(R"(\b(\?[a-zA-Z0-9_@$]+)\b)");
    std::map<std::string, std::string> demangle_cache;
    
    auto words_begin = std::sregex_iterator(code.begin(), code.end(), mangled_regex);
    auto words_end = std::sregex_iterator();

    for (std::sregex_iterator i = words_begin; i != words_end; ++i) {
        std::string mangled = i->str();
        if (demangle_cache.count(mangled)) continue;

        char demangled[1024];
        DWORD result_len = UnDecorateSymbolName(
            mangled.c_str(), demangled, sizeof(demangled),
            UNDNAME_COMPLETE | UNDNAME_NO_ACCESS_SPECIFIERS
        );
        if (result_len > 0) {
            std::string demangled_str(demangled);
            
            // Simplify: remove full signature for function name replacement
            std::string simplified = demangled_str;
            size_t paren = simplified.find('(');
            if (paren != std::string::npos) {
                simplified = simplified.substr(0, paren);
            }
            
            demangle_cache[mangled] = simplified;
        }
    }
#else
    // GCC/Clang: Demangle _Z-prefixed symbols using __cxa_demangle
    std::regex mangled_regex(R"(\b(_Z[a-zA-Z0-9_]+)\b)");
    std::map<std::string, std::string> demangle_cache;
    
    auto words_begin = std::sregex_iterator(code.begin(), code.end(), mangled_regex);
    auto words_end = std::sregex_iterator();

    for (std::sregex_iterator i = words_begin; i != words_end; ++i) {
        std::string mangled = i->str();
        if (demangle_cache.count(mangled)) continue;

        int status = 0;
        char* demangled = abi::__cxa_demangle(mangled.c_str(), nullptr, nullptr, &status);
        if (status == 0 && demangled != nullptr) {
            std::string demangled_str(demangled);
            
            // Simplify: remove full signature for function name replacement
            // e.g. "Circle::area() const" -> "Circle::area"
            std::string simplified = demangled_str;
            size_t paren = simplified.find('(');
            if (paren != std::string::npos) {
                simplified = simplified.substr(0, paren);
            }
            
            demangle_cache[mangled] = simplified;
            free(demangled);
        }
    }
#endif

    for (const auto& [mangled, demangled] : demangle_cache) {
        size_t pos = 0;
        while ((pos = result.find(mangled, pos)) != std::string::npos) {
            result.replace(pos, mangled.length(), demangled);
            pos += demangled.length();
        }
    }

    // 2. Standardize 'this' pointer for member functions
    // Find function headers like "Type Class::Func(..., longlong param_1, ...)"
    // And also replace param_1 with 'this' in the body of those functions.
    
    std::regex member_func_regex(R"(\b([a-zA-Z0-9_]+::[a-zA-Z0-9_~]+)\s*\(([^\)]*)\))");
    
    std::string final_code;
    size_t last_pos = 0;
    
    auto headers_begin = std::sregex_iterator(result.begin(), result.end(), member_func_regex);
    auto headers_end = std::sregex_iterator();

    for (std::sregex_iterator i = headers_begin; i != headers_end; ++i) {
        std::smatch match = *i;
        final_code += result.substr(last_pos, match.position() - last_pos);
        
        std::string full_header = match.str();
        std::string class_func = match[1].str();
        std::string params = match[2].str();
        
        // Find className from class_func
        size_t colon_pos = class_func.find("::");
        std::string class_name = (colon_pos != std::string::npos) ? class_func.substr(0, colon_pos) : "";

        bool has_this = false;
        // Check for param_1 (the 'this' pointer)
        size_t p1_pos = params.find("param_1");
        if (p1_pos != std::string::npos && !class_name.empty()) {
            // Check if it's the first param or has a type
            params.replace(p1_pos, 7, "this");
            
            // Heuristic attempt to update the type to ClassName*
            size_t type_pos = params.rfind(' ', p1_pos);
            if (type_pos != std::string::npos) {
                // Determine start of type
                size_t start = params.rfind(',', type_pos);
                if (start == std::string::npos) start = 0;
                else start++;
                
                // Skip spaces
                while(start < params.length() && isspace(params[start])) start++;
                
                if (start < type_pos) {
                    params.replace(start, type_pos - start, class_name + " *");
                }
            }
            has_this = true;
        }

        std::string new_header = class_func + "(" + params + ")";
        final_code += new_header;
        
        // Find the scope of this function to replace param_1 in body
        size_t body_start = result.find('{', match.position() + match.length());
        if (body_start != std::string::npos && has_this) {
            // Find corresponding '}' - simplistic but better than global
            int depth = 1;
            size_t body_end = body_start + 1;
            while (body_end < result.length() && depth > 0) {
                if (result[body_end] == '{') depth++;
                else if (result[body_end] == '}') depth--;
                body_end++;
            }
            
            if (depth == 0) {
                std::string body = result.substr(body_start, body_end - body_start);
                // Replace \bparam_1\b with this
                body = std::regex_replace(body, std::regex(R"(\bparam_1\b)"), "this");
                
                final_code += result.substr(match.position() + match.length(), body_start - (match.position() + match.length()));
                final_code += body;
                last_pos = body_end;
            } else {
                last_pos = match.position() + match.length();
            }
        } else {
            last_pos = match.position() + match.length();
        }
    }
    final_code += result.substr(last_pos);

    return final_code.empty() ? result : final_code;
}

std::string normalize_cpp_virtual_calls(const std::string& code) {
    if (code.empty()) return code;

    std::string result = code;

    // Pattern: (**(code **)(*obj + 0x10))(args);
    // Add lightweight semantic annotations for common vtable slots.
    static const std::regex vcall_pattern(
        R"(\(\*\*\(code \*\*\)\(\*(\w+) \+ (0x[0-9a-fA-F]+|\d+)\)\)\(([^)]*)\);)"
    );

    std::string rewritten;
    rewritten.reserve(result.size() + 64);

    size_t cursor = 0;
    auto begin = std::sregex_iterator(result.begin(), result.end(), vcall_pattern);
    auto end = std::sregex_iterator();
    for (auto it = begin; it != end; ++it) {
        const std::smatch& m = *it;
        size_t pos = static_cast<size_t>(m.position());
        size_t len = static_cast<size_t>(m.length());

        rewritten.append(result, cursor, pos - cursor);

        const std::string obj = m[1].str();
        const std::string off = m[2].str();
        std::string args = m[3].str();

        std::string annotation = "virtual call";
        if (off == "8" || off == "0x8" || off == "0X8") {
            annotation = "virtual dtor";
        } else if (off == "16" || off == "0x10" || off == "0X10") {
            annotation = "virtual method";
        }

        if ((annotation == "virtual dtor") && args.empty()) {
            args = obj;
        }

        std::ostringstream oss;
        oss << "/* " << annotation << " @" << off << " */ "
            << "(**(code **)(*" << obj << " + " << off << "))(" << args << ");";
        rewritten += oss.str();
        cursor = pos + len;
    }
    rewritten.append(result, cursor, std::string::npos);
    result.swap(rewritten);

    return result;
}

std::string normalize_cpp_virtual_calls(
    const std::string& code,
    const std::map<uint64_t, std::map<int, std::string>>& vtable_virtual_names,
    const std::map<int, std::string>& vcall_slot_name_hints,
    const std::map<int, uint64_t>& vcall_slot_target_hints
) {
    // First apply vtable-context-aware renaming using the slot hints
    std::string result = code;

    // For each known slot offset with a resolved name, annotate matching patterns
    for (const auto& [slot_offset, name] : vcall_slot_name_hints) {
        // Build hex representations of the slot offset
        char hex_off[16];
        snprintf(hex_off, sizeof(hex_off), "0x%x", slot_offset);

        // Pattern: (**(code **)(*obj + OFFSET))(args);
        // We look for the offset in the code and add the resolved name as a comment
        std::string pattern = std::string("+ ") + hex_off + "))";
        size_t pos = 0;
        while ((pos = result.find(pattern, pos)) != std::string::npos) {
            // Check if this is inside a virtual call pattern (look for "code **" before)
            size_t check_start = (pos > 40) ? pos - 40 : 0;
            std::string prefix = result.substr(check_start, pos - check_start);
            if (prefix.find("code **") != std::string::npos) {
                // Find the end of the call (the next semicolon)
                size_t semi = result.find(';', pos);
                if (semi != std::string::npos) {
                    // Insert comment after the semicolon
                    std::string comment = " /* " + name + " */";
                    // Check if already annotated
                    if (result.substr(semi + 1, 3) != " /*") {
                        result.insert(semi + 1, comment);
                        pos = semi + 1 + comment.length();
                        continue;
                    }
                }
            }
            pos += pattern.length();
        }
    }

    (void)vtable_virtual_names;     // Available for future use
    (void)vcall_slot_target_hints;  // Available for future use

    // Then apply the generic regex-based normalization
    return normalize_cpp_virtual_calls(result);
}

} // namespace processing
} // namespace fission