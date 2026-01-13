#include "fission/decompiler/PostProcessor.h"
#include "fission/decompiler/CFGStructurizer.h"
#include <vector>
#include <cctype>
#include <regex>
#include <map>
#include <sstream>
#include <algorithm>

namespace fission {
namespace decompiler {

std::string PostProcessor::convert_integer_constants(std::string c_code) {
    // Manual scan for hex patterns: 0x[0-9a-fA-F]+
    size_t pos = 0;
    while ((pos = c_code.find("0x", pos)) != std::string::npos) {
        size_t start = pos;
        size_t end = start + 2;
        while (end < c_code.length() && isxdigit(c_code[end])) {
            end++;
        }
        
        size_t len = end - start;
        if (len > 4) { // Only check if longer than single byte (0xXX)
            std::string hex_str = c_code.substr(start, len);
            try {
                unsigned long long val = std::stoull(hex_str, nullptr, 16);
                
                // Extract bytes (Little Endian for x86)
                std::string decoded;
                bool is_ascii = true;
                unsigned long long temp = val;
                
                std::vector<char> bytes;
                while (temp > 0) {
                    char c = (char)(temp & 0xFF);
                    bytes.push_back(c);
                    temp >>= 8;
                }
                
                // If empty, it was 0x0 - not a string
                if (bytes.empty()) is_ascii = false;
                
                int valid_chars = 0;
                for (char c : bytes) {
                    if (c == 0) continue; // Allow null terminators
                    if (isalnum(c) || ispunct(c) || c == ' ') {
                        valid_chars++;
                        decoded += c;
                    } else {
                        is_ascii = false;
                        break;
                    }
                }
                
                if (is_ascii && valid_chars >= 3) {
                    // Found a string! Format: (QWORD)"string" or (DWORD)"string"
                    std::string replacement = "\"" + decoded + "\"";
                    if (len > 10) { // > 4 bytes -> QWORD
                        replacement = "(QWORD)" + replacement;
                    } else {
                        replacement = "(DWORD)" + replacement;
                    }
                    
                    c_code.replace(start, len, replacement);
                    pos = start + replacement.length();
                    continue;
                }
            } catch (...) {
                // Ignore conversion errors
            }
        }
        pos = end;
    }
    return c_code;
}

std::string PostProcessor::convert_while_to_for(std::string c_code) {
    // Convert compound assignment operators
    
    // Convert "i = i + 1" to "i++"
    std::regex increment_pattern(R"((\w+)\s*=\s*\1\s*\+\s*1\s*;)");
    c_code = std::regex_replace(c_code, increment_pattern, "$1++;");
    
    // Convert "i = i - 1" to "i--"
    std::regex decrement_pattern(R"((\w+)\s*=\s*\1\s*-\s*1\s*;)");
    c_code = std::regex_replace(c_code, decrement_pattern, "$1--;");
    
    // Convert "i = i + N" to "i += N"
    std::regex add_assign_pattern(R"((\w+)\s*=\s*\1\s*\+\s*([^;]+);)");
    c_code = std::regex_replace(c_code, add_assign_pattern, "$1 += $2;");
    
    // Convert "i = i - N" to "i -= N"
    std::regex sub_assign_pattern(R"((\w+)\s*=\s*\1\s*-\s*([^;]+);)");
    c_code = std::regex_replace(c_code, sub_assign_pattern, "$1 -= $2;");
    
    // Convert "i = i * N" to "i *= N"
    std::regex mul_assign_pattern(R"((\w+)\s*=\s*\1\s*\*\s*([^;]+);)");
    c_code = std::regex_replace(c_code, mul_assign_pattern, "$1 *= $2;");
    
    // Convert "i = i | N" to "i |= N"
    std::regex or_assign_pattern(R"((\w+)\s*=\s*\1\s*\|\s*([^;]+);)");
    c_code = std::regex_replace(c_code, or_assign_pattern, "$1 |= $2;");
    
    // Convert "i = i & N" to "i &= N"
    std::regex and_assign_pattern(R"((\w+)\s*=\s*\1\s*\&\s*([^;]+);)");
    c_code = std::regex_replace(c_code, and_assign_pattern, "$1 &= $2;");
    
    return c_code;
}

std::string PostProcessor::simplify_nested_if(std::string c_code) {
    // Remove redundant parentheses in conditions
    std::regex double_paren(R"(\(\(([^()]+)\)\))");
    c_code = std::regex_replace(c_code, double_paren, "($1)");
    
    // Simplify "if (x != 0)" to "if (x)"
    std::regex non_zero_check(R"(if\s*\(\s*(\w+)\s*!=\s*0\s*\))");
    c_code = std::regex_replace(c_code, non_zero_check, "if ($1)");
    
    // Simplify "if (x == 0)" to "if (!x)"
    std::regex zero_check(R"(if\s*\(\s*(\w+)\s*==\s*0\s*\))");
    c_code = std::regex_replace(c_code, zero_check, "if (!$1)");
    
    return c_code;
}

std::string PostProcessor::fold_array_init(std::string c_code) {
    // Pattern detection for sequential local variable assignments
    // For now, just return the original code
    // Full implementation would replace with array initializers
    return c_code;
}

std::string PostProcessor::improve_variable_names(std::string c_code) {
    // If a variable is returned, rename it to "result"
    std::regex return_var_pattern(R"(return\s+(local_\w+)\s*;)");
    std::smatch match;
    
    if (std::regex_search(c_code, match, return_var_pattern)) {
        std::string var_name = match[1].str();
        if (var_name.find("local_") == 0) {
            // Count occurrences - only rename if used more than once
            std::regex var_pattern(var_name);
            auto begin = std::sregex_iterator(c_code.begin(), c_code.end(), var_pattern);
            auto end = std::sregex_iterator();
            int count = std::distance(begin, end);
            
            if (count >= 2 && count <= 10) {
                c_code = std::regex_replace(c_code, var_pattern, "result");
            }
        }
    }
    
    return c_code;
}

std::string PostProcessor::structurize_control_flow(std::string c_code) {
    // Use CFGStructurizer for goto elimination and loop normalization
    return CFGStructurizer::structurize(c_code);
}

std::string PostProcessor::process(const std::string& c_code) {
    std::string result = c_code;
    
    // Apply all optimization passes in order
    // 1. Extract string literals from integer constants
    result = convert_integer_constants(result);
    
    // 2. Structurize control flow (eliminate gotos, normalize loops)
    result = structurize_control_flow(result);
    
    // 3. Convert to compound operators (i++ etc)
    result = convert_while_to_for(result);
    
    // 4. Simplify conditions
    result = simplify_nested_if(result);
    
    // 5. Detect array initializations
    result = fold_array_init(result);
    
    // 6. Improve variable names
    result = improve_variable_names(result);
    
    return result;
}

} // namespace decompiler
} // namespace fission

