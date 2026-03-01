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
    // Static regex objects — compiled once at first call (C++11 magic statics)
    static const std::regex increment_pattern(R"((\.\w+|\w+)\s*=\s*\1\s*\+\s*1\s*;)");
    static const std::regex decrement_pattern(R"((\.\w+|\w+)\s*=\s*\1\s*-\s*1\s*;)");
    static const std::regex add_assign_pattern(R"((\w+)\s*=\s*\1\s*\+\s*([^;]+);)");
    static const std::regex sub_assign_pattern(R"((\w+)\s*=\s*\1\s*-(?!>)\s*([^;]+);)");
    static const std::regex mul_assign_pattern(R"((\w+)\s*=\s*\1\s*\*\s*([^;]+);)");
    static const std::regex or_assign_pattern(R"((\w+)\s*=\s*\1\s*\|\s*([^;]+);)");
    static const std::regex and_assign_pattern(R"((\w+)\s*=\s*\1\s*\&\s*([^;]+);)");

    c_code = std::regex_replace(c_code, increment_pattern, "$1++;");
    c_code = std::regex_replace(c_code, decrement_pattern, "$1--;");
    c_code = std::regex_replace(c_code, add_assign_pattern, "$1 += $2;");
    c_code = std::regex_replace(c_code, sub_assign_pattern, "$1 -= $2;");
    c_code = std::regex_replace(c_code, mul_assign_pattern, "$1 *= $2;");
    c_code = std::regex_replace(c_code, or_assign_pattern, "$1 |= $2;");
    c_code = std::regex_replace(c_code, and_assign_pattern, "$1 &= $2;");
    return c_code;
}

std::string PostProcessor::simplify_nested_if(std::string c_code) {
    static const std::regex double_paren(R"(\(\(([^()]+)\)\))");
    static const std::regex non_zero_check(R"(if\s*\(\s*(\w+)\s*!=\s*0\s*\))");
    static const std::regex zero_check(R"(if\s*\(\s*(\w+)\s*==\s*0\s*\))");

    c_code = std::regex_replace(c_code, double_paren, "($1)");
    c_code = std::regex_replace(c_code, non_zero_check, "if ($1)");
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
    static const std::regex return_var_pattern(R"(return\s+(local_\w+)\s*;)");
    std::smatch match;

    if (std::regex_search(c_code, match, return_var_pattern)) {
        std::string var_name = match[1].str();
        if (var_name.rfind("local_", 0) == 0) {
            // Build a plain-text pattern for this specific variable name
            // (var_name contains only \w chars, so no escaping needed)
            const std::regex var_pattern(var_name);
            auto it  = std::sregex_iterator(c_code.begin(), c_code.end(), var_pattern);
            int count = static_cast<int>(std::distance(it, std::sregex_iterator()));
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

std::string PostProcessor::convert_while_to_for_struct(std::string c_code) {
    // Static regex objects — compiled once at first call (C++11 magic statics)
    static const std::regex while_pat(
        R"(^(\s*)while\s*\(\s*(\w+)\s*(<=|<|>=|>|!=|==)\s*([^)]+)\)\s*\{\s*$)");
    static const std::regex init_pat(R"(^(\s*)(\w+)\s*=\s*([^;]+);\s*$)");
    // Increment patterns — checked after compound-operator pass, so ++ / -- etc. are normalised
    static const std::regex inc_pp(R"(^(\s*)(\w+)\s*\+\+\s*;\s*$)");
    static const std::regex inc_mm(R"(^(\s*)(\w+)\s*--\s*;\s*$)");
    static const std::regex inc_pe(R"(^(\s*)(\w+)\s*\+=\s*([^;]+);\s*$)");
    static const std::regex inc_me(R"(^(\s*)(\w+)\s*-=\s*([^;]+);\s*$)");
    // Pre-normalised fallbacks: var = var + expr  /  var = var - expr
    static const std::regex inc_raw_plus(R"(^(\s*)(\w+)\s*=\s*(\w+)\s*\+\s*([^;]+);\s*$)");
    static const std::regex inc_raw_minus(R"(^(\s*)(\w+)\s*=\s*(\w+)\s*-(?!>)\s*([^;]+);\s*$)");

    // Split into lines (preserve trailing newline flag)
    bool ends_nl = !c_code.empty() && c_code.back() == '\n';
    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    bool any_changed = true;
    while (any_changed) {
        any_changed = false;
        for (size_t i = 0; i < lines.size(); i++) {
            std::smatch wm;
            if (!std::regex_match(lines[i], wm, while_pat)) continue;

            std::string w_indent = wm[1].str();
            std::string w_var   = wm[2].str();
            std::string w_op    = wm[3].str();
            std::string w_end   = wm[4].str();
            // trim trailing whitespace from w_end
            while (!w_end.empty() && std::isspace((unsigned char)w_end.back()))
                w_end.pop_back();

            // The line immediately before must assign w_var
            if (i == 0) continue;
            std::smatch im;
            if (!std::regex_match(lines[i - 1], im, init_pat)) continue;
            if (im[2].str() != w_var) continue;
            std::string init_val = im[3].str();
            while (!init_val.empty() && std::isspace((unsigned char)init_val.back()))
                init_val.pop_back();

            // Find the matching closing brace
            int depth = 1;
            size_t j = i + 1;
            while (j < lines.size() && depth > 0) {
                for (char c : lines[j]) {
                    if (c == '{') depth++;
                    else if (c == '}') { if (--depth == 0) break; }
                }
                if (depth > 0) j++;
            }
            if (j >= lines.size() || depth != 0) continue;
            // j == index of closing '}' line

            // Body must have at least one line (the increment)
            if (j <= i + 1) continue;
            size_t inc_idx = j - 1;

            // Detect increment of w_var on inc_idx
            std::smatch incm;
            std::string inc_str;
            bool ok = false;
            if (std::regex_match(lines[inc_idx], incm, inc_pp) && incm[2].str() == w_var) {
                inc_str = w_var + "++";
                ok = true;
            } else if (std::regex_match(lines[inc_idx], incm, inc_mm) && incm[2].str() == w_var) {
                inc_str = w_var + "--";
                ok = true;
            } else if (std::regex_match(lines[inc_idx], incm, inc_pe) && incm[2].str() == w_var) {
                inc_str = w_var + " += " + incm[3].str();
                while (!inc_str.empty() && std::isspace((unsigned char)inc_str.back()))
                    inc_str.pop_back();
                ok = true;
            } else if (std::regex_match(lines[inc_idx], incm, inc_me) && incm[2].str() == w_var) {
                inc_str = w_var + " -= " + incm[3].str();
                while (!inc_str.empty() && std::isspace((unsigned char)inc_str.back()))
                    inc_str.pop_back();
                ok = true;
            } else if (std::regex_match(lines[inc_idx], incm, inc_raw_plus)
                       && incm[2].str() == w_var && incm[3].str() == w_var) {
                std::string step = incm[4].str();
                while (!step.empty() && std::isspace((unsigned char)step.back())) step.pop_back();
                inc_str = (step == "1") ? (w_var + "++") : (w_var + " += " + step);
                ok = true;
            } else if (std::regex_match(lines[inc_idx], incm, inc_raw_minus)
                       && incm[2].str() == w_var && incm[3].str() == w_var) {
                std::string step = incm[4].str();
                while (!step.empty() && std::isspace((unsigned char)step.back())) step.pop_back();
                inc_str = (step == "1") ? (w_var + "--") : (w_var + " -= " + step);
                ok = true;
            }
            if (!ok) continue;

            // Build replacement: remove init line (i-1) and increment line (inc_idx)
            std::string for_line = w_indent + "for (" + w_var + " = " + init_val + "; "
                                 + w_var + " " + w_op + " " + w_end + "; " + inc_str + ") {";

            std::vector<std::string> newlines;
            newlines.reserve(lines.size());
            for (size_t k = 0; k < i - 1; k++)  newlines.push_back(lines[k]);  // before init
            newlines.push_back(for_line);                                        // for (...) {
            for (size_t k = i + 1; k < inc_idx; k++) newlines.push_back(lines[k]); // body
            newlines.push_back(w_indent + "}");                                  // closing brace
            for (size_t k = j + 1; k < lines.size(); k++) newlines.push_back(lines[k]); // rest

            lines = std::move(newlines);
            any_changed = true;
            break;  // restart scan
        }
    }

    std::string out;
    out.reserve(c_code.size());
    for (size_t i = 0; i < lines.size(); i++) {
        out += lines[i];
        if (i + 1 < lines.size()) out += '\n';
    }
    if (ends_nl && (out.empty() || out.back() != '\n')) out += '\n';
    return out;
}

std::string PostProcessor::process(const std::string& c_code) {
    std::string result = c_code;
    
    // Apply all optimization passes in order
    // 1. Extract string literals from integer constants
    result = convert_integer_constants(result);
    
    // 2. Structurize control flow (eliminate gotos, normalize loops)
    result = structurize_control_flow(result);

    // 3. Convert to compound operators (i++ etc) — must run before while→for struct pass
    result = convert_while_to_for(result);

    // 4. Convert while+init+increment → for loops (sees already-normalised ++ / -- etc.)
    result = convert_while_to_for_struct(result);

    // 5. Simplify conditions
    result = simplify_nested_if(result);
    
    // 6. Detect array initializations
    result = fold_array_init(result);
    
    // 7. Improve variable names
    result = improve_variable_names(result);
    
    return result;
}

} // namespace decompiler
} // namespace fission

