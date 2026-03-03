#include "fission/processing/PostProcessors.h"

#include <string>
#include <map>
#include <set>
#include <vector>
#include <regex>
#include <algorithm>
#include <sstream>
#include <cctype>
#include <cstring>

namespace fission {
namespace processing {

// ============================================================================
// Shadow Parameter Stripping (Windows x64 MSVC ABI)
// ============================================================================

std::string strip_shadow_only_params(const std::string& code) {
    // Locate the opening brace that begins the function body.
    // We cannot use code.find('{') because the code may be prefixed by
    // "// Inferred Structure Definitions\ntypedef struct name { ... } name;"
    // blocks whose '{' would be found first.
    // The function body '{' always immediately follows the closing ')' of the
    // function signature (possibly separated by whitespace/newlines), whereas
    // struct braces follow an identifier, not ')'.
    static const std::regex body_brace_re(R"(\)\s*(\{))", std::regex::optimize);
    std::smatch sm;
    if (!std::regex_search(code, sm, body_brace_re)) return code;
    size_t brace_pos = (size_t)sm.position(1);  // position of the body '{'

    std::string header = code.substr(0, brace_pos);
    std::string body   = code.substr(brace_pos);

    // Find the parameter list: last '(' ... ')' pair in the header.
    size_t paren_open  = header.rfind('(');
    size_t paren_close = header.rfind(')');
    if (paren_open  == std::string::npos ||
        paren_close == std::string::npos ||
        paren_close < paren_open) {
        return code;
    }

    std::string pre_params     = header.substr(0, paren_open + 1);  // up to and including '('
    std::string param_list_str = header.substr(paren_open + 1, paren_close - paren_open - 1);
    std::string post_params    = header.substr(paren_close);        // from ')' onward

    // Collect all param_N identifiers that are actually referenced in the body.
    std::set<std::string> used_in_body;
    {
        static const std::regex param_re(R"(\bparam_\d+\b)", std::regex::optimize);
        auto it  = std::sregex_iterator(body.begin(), body.end(), param_re);
        auto end = std::sregex_iterator();
        for (; it != end; ++it) {
            used_in_body.insert((*it)[0].str());
        }
    }

    // Split the parameter list on commas and drop shadow-only parameters.
    static const std::regex param_name_re(R"(\bparam_\d+\b)", std::regex::optimize);
    std::vector<std::string> kept_params;

    std::istringstream ss(param_list_str);
    std::string token;
    while (std::getline(ss, token, ',')) {
        // Trim surrounding whitespace.
        auto first = token.find_first_not_of(" \t\n\r");
        auto last  = token.find_last_not_of(" \t\n\r");
        if (first == std::string::npos) continue;
        token = token.substr(first, last - first + 1);

        // If this token contains a param_N that is not used in the body, skip it.
        std::smatch m;
        if (std::regex_search(token, m, param_name_re)) {
            if (used_in_body.count(m[0].str()) == 0) {
                continue;  // shadow-only → drop
            }
        }
        kept_params.push_back(token);
    }

    // If nothing was dropped, return the original to avoid spurious copies.
    if (kept_params.size() == param_list_str.find(',') + 1 &&
        kept_params.size() > 0) {
        // Quick check: count original comma-separated tokens
        size_t orig_count = 1;
        for (char c : param_list_str) if (c == ',') ++orig_count;
        if (kept_params.size() == orig_count) return code;
    }

    // Reconstruct the function signature with remaining parameters.
    std::string new_param_list;
    for (size_t i = 0; i < kept_params.size(); ++i) {
        if (i > 0) new_param_list += ", ";
        new_param_list += kept_params[i];
    }

    return pre_params + new_param_list + post_params + body;
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
// Structure Access Conversion (pointer arithmetic → arrow notation)
// ============================================================================

// Helper: parse struct typedef definitions from code header
static std::map<std::string, std::map<int, std::string>>
parse_struct_typedefs(const std::string& code) {
    std::map<std::string, std::map<int, std::string>> structs;
    std::regex typedef_re(R"(typedef\s+struct\s+(\w+)\s*\{([^}]*)\}\s*(\w+)\s*;)");
    std::regex field_re(R"((\w[\w\s\*]*)\s+(\w+)\s*;\s*//\s*Offset\s+([0-9a-fA-Fx]+))");

    auto tbegin = std::sregex_iterator(code.begin(), code.end(), typedef_re);
    auto tend   = std::sregex_iterator();

    for (auto it = tbegin; it != tend; ++it) {
        std::string struct_name = (*it)[3].str();
        std::string body = (*it)[2].str();

        std::map<int, std::string> fields;
        auto fbegin = std::sregex_iterator(body.begin(), body.end(), field_re);
        auto fend   = std::sregex_iterator();
        for (auto fi = fbegin; fi != fend; ++fi) {
            std::string fname = (*fi)[2].str();
            std::string off_str = (*fi)[3].str();
            int offset = 0;
            try {
                if (off_str.size() > 2 && off_str.substr(0, 2) == "0x") {
                    offset = std::stoi(off_str.substr(2), nullptr, 16);
                } else {
                    offset = std::stoi(off_str);
                }
            } catch (...) { continue; }
            fields[offset] = fname;
        }
        if (!fields.empty()) {
            structs[struct_name] = std::move(fields);
        }
    }
    return structs;
}

// Helper: find struct-typed parameters
static std::map<std::string, std::string>
find_struct_params(const std::string& code,
                   const std::map<std::string, std::map<int, std::string>>& struct_defs) {
    std::map<std::string, std::string> params;
    for (auto const& [sname, _] : struct_defs) {
        std::regex param_re(sname + R"(\s*\*\s*(\w+))");
        auto pbegin = std::sregex_iterator(code.begin(), code.end(), param_re);
        auto pend   = std::sregex_iterator();
        for (auto pi = pbegin; pi != pend; ++pi) {
            std::string var_name = (*pi)[1].str();
            params[var_name] = sname;
        }
    }
    return params;
}

// Helper: parse a hex or decimal offset string to int, returns -1 on failure
static int parse_offset_value(const std::string& s) {
    if (s.empty()) return -1;
    try {
        if (s.size() > 2 && (s.substr(0,2) == "0x" || s.substr(0,2) == "0X")) {
            return std::stoi(s.substr(2), nullptr, 16);
        }
        return std::stoi(s);
    } catch (...) { return -1; }
}

std::string annotate_structure_offsets(const std::string& code) {
    return code;
}

std::string annotate_structure_offsets(const std::string& code,
                                       const std::map<std::string, std::string>& type_replacements) {
    if (code.empty()) return code;

    std::string result = code;

    // Phase 1: Build field mapping from type_replacements
    std::map<int, std::string> tr_fields;
    for (auto const& [key, value] : type_replacements) {
        if (key.substr(0, 5) == "@off:") {
            std::string off_str = key.substr(5);
            int offset = parse_offset_value(off_str);
            if (offset < 0) continue;
            std::string field_name = value;
            size_t dot = value.find('.');
            if (dot != std::string::npos) field_name = value.substr(dot + 1);
            tr_fields[offset] = field_name;
        }
    }

    // Phase 2: Parse struct typedefs from code header
    auto struct_defs = parse_struct_typedefs(result);
    auto struct_params = find_struct_params(result, struct_defs);

    // Helper lambda: look up field name by byte offset for a given variable
    auto resolve_field = [&](const std::string& var_name, int byte_offset) -> std::string {
        if (struct_params.count(var_name)) {
            auto const& sname = struct_params.at(var_name);
            if (struct_defs.count(sname) && struct_defs.at(sname).count(byte_offset)) {
                return struct_defs.at(sname).at(byte_offset);
            }
        }
        if (byte_offset >= 0 && tr_fields.count(byte_offset)) {
            if (var_name.substr(0, 6) == "param_" ||
                var_name.substr(0, 6) == "local_" ||
                var_name.find("Var") != std::string::npos) {
                return tr_fields[byte_offset];
            }
        }
        return "";
    };

    // Helper lambda: compute struct element size
    auto detect_element_size = [&](const std::string& var_name) -> int {
        if (struct_params.count(var_name)) {
            auto const& sname = struct_params.at(var_name);
            if (struct_defs.count(sname)) {
                auto const& fields = struct_defs.at(sname);
                for (auto const& [off, _] : fields) {
                    if (off > 0) return off;
                }
            }
        }
        return 4;
    };

    // Phase 3: Convert *(TYPE *)(VAR + OFFSET) → VAR->field_name
    std::regex access_re(
        R"(\*\s*\(\s*(\w[\w\s]*\*)\s*\)\s*\()"
        R"(\s*(?:\([^)]*\)\s*)?)"
        R"((\w+))"
        R"(\s*\+\s*)"
        R"((0x[0-9a-fA-F]+|\d+))"
        R"(\s*\))"
    );

    std::string converted;
    converted.reserve(result.size());
    std::string::const_iterator search_start = result.cbegin();
    std::smatch m;

    while (std::regex_search(search_start, result.cend(), m, access_re)) {
        converted.append(search_start, search_start + m.position());

        std::string cast_type = m[1].str();
        std::string var_name = m[2].str();
        std::string offset_str = m[3].str();
        int byte_offset = parse_offset_value(offset_str);

        std::string field_name = resolve_field(var_name, byte_offset);

        if (!field_name.empty()) {
            converted += var_name + "->" + field_name;
        } else {
            converted.append(m[0].str());
        }

        search_start += m.position() + m.length();
    }
    converted.append(search_start, result.cend());
    result = std::move(converted);

    // Phase 4: Convert VAR[N] → VAR->field_{N*elem_size}
    {
        if (!struct_params.empty()) {
            std::regex arr_idx_re(R"((\b(?:param_\d+|local_\w+)\b)\s*\[\s*(\d+|0x[0-9a-fA-F]+)\s*\])");

            std::string phase4;
            phase4.reserve(result.size());
            std::string::const_iterator s4 = result.cbegin();
            std::smatch m4;

            while (std::regex_search(s4, result.cend(), m4, arr_idx_re)) {
                phase4.append(s4, s4 + m4.position());

                std::string var_name = m4[1].str();
                std::string idx_str = m4[2].str();
                int idx = parse_offset_value(idx_str);

                if (idx >= 0 && struct_params.count(var_name)) {
                    int elem_size = detect_element_size(var_name);
                    int byte_offset = idx * elem_size;
                    std::string field = resolve_field(var_name, byte_offset);

                    if (!field.empty()) {
                        phase4 += var_name + "->" + field;
                    } else {
                        phase4.append(m4[0].str());
                    }
                } else {
                    phase4.append(m4[0].str());
                }

                s4 += m4.position() + m4.length();
            }
            phase4.append(s4, result.cend());
            result = std::move(phase4);
        }
    }

    // Phase 5: Convert *VAR → VAR->field_0
    {
        if (!struct_params.empty()) {
            std::regex deref_re(R"(\*\s*(param_\d+|local_\w+)\b)");

            std::string phase5;
            phase5.reserve(result.size());
            std::string::const_iterator s5 = result.cbegin();
            std::smatch m5;

            while (std::regex_search(s5, result.cend(), m5, deref_re)) {
                size_t match_pos_in_result =
                    static_cast<size_t>(std::distance(result.cbegin(), s5)) + m5.position();

                bool in_declaration = false;
                if (match_pos_in_result > 0) {
                    size_t scan = match_pos_in_result - 1;
                    while (scan > 0 &&
                           (result[scan] == ' ' || result[scan] == '\t')) {
                        --scan;
                    }
                    char prev_char = result[scan];
                    if (std::isalnum(static_cast<unsigned char>(prev_char)) ||
                        prev_char == '_') {
                        in_declaration = true;
                    }
                }

                bool followed_by_bracket = false;
                {
                    size_t after = match_pos_in_result + m5.length();
                    while (after < result.size() &&
                           (result[after] == ' ' || result[after] == '\t')) {
                        ++after;
                    }
                    if (after < result.size() && result[after] == '[') {
                        followed_by_bracket = true;
                    }
                }

                phase5.append(s5, s5 + m5.position());

                std::string var_name = m5[1].str();

                if (!in_declaration && !followed_by_bracket &&
                    struct_params.count(var_name)) {
                    std::string field = resolve_field(var_name, 0);
                    if (!field.empty()) {
                        phase5 += var_name + "->" + field;
                    } else {
                        phase5.append(m5[0].str());
                    }
                } else {
                    phase5.append(m5[0].str());
                }

                s5 += m5.position() + m5.length();
            }
            phase5.append(s5, result.cend());
            result = std::move(phase5);
        }
    }

    return result;
}

} // namespace processing
} // namespace fission
