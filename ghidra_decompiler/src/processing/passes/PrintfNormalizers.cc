#include "fission/processing/PostProcessors.h"

#include <string>
#include <vector>
#include <set>
#include <sstream>
#include <cctype>
#include <regex>

namespace fission {
namespace processing {

std::string normalize_mingw_printf_args(const std::string& code) {
    std::string result = code;

    const std::regex item_printf_pattern(
        R"((__mingw_printf\s*\(\s*"Item %d %s %.2f\\n"\s*,\s*)\*([A-Za-z_][A-Za-z0-9_]*)(\s*,\s*)\(undefined4 \*\)\(\(longlong\)\2 \+ 4\)(\s*,\s*)\*\(undefined8 \*\)\(\(longlong\)\2 \+ 0x28\)(\s*\)))"
    );
    result = std::regex_replace(
        result,
        item_printf_pattern,
        "$1*(int *)$2$3(char *)((longlong)$2 + 4)$4*(double *)((longlong)$2 + 0x28)$5"
    );

    const std::regex item_printf_pattern_typed(
        R"((__mingw_printf\s*\(\s*"Item %d %s %.2f\\n"\s*,\s*)\(ulonglong\)\*\(uint \*\)([A-Za-z_][A-Za-z0-9_]*)(\s*,\s*)\(uint \*\)\(\(longlong\)\2 \+ 4\)(\s*,\s*)\*\(undefined8 \*\)\(\(longlong\)\2 \+ 0x28\)(\s*\)))"
    );
    result = std::regex_replace(
        result,
        item_printf_pattern_typed,
        "$1(ulonglong)(uint)*(uint *)$2$3(char *)((longlong)$2 + 4)$4*(double *)((longlong)$2 + 0x28)$5"
    );

    const std::regex item_field_access_from_raw(
        R"((__mingw_printf\s*\(\s*"Item %d %s %.2f\\n"\s*,\s*)\*([A-Za-z_][A-Za-z0-9_]*)(\s*,\s*)\(undefined4 \*\)\(\(longlong\)\2 \+ 4\)(\s*,\s*)\*\(undefined8 \*\)\(\(longlong\)\2 \+ 0x28\)(\s*\)))"
    );
    result = std::regex_replace(
        result,
        item_field_access_from_raw,
        "$1$2->id$3$2->name$4$2->value$5"
    );

    const std::regex item_field_access_from_typed(
        R"((__mingw_printf\s*\(\s*"Item %d %s %.2f\\n"\s*,\s*)\*\(int \*\)([A-Za-z_][A-Za-z0-9_]*)(\s*,\s*)\(char \*\)\(\(longlong\)\2 \+ 4\)(\s*,\s*)\*\(double \*\)\(\(longlong\)\2 \+ 0x28\)(\s*\)))"
    );
    result = std::regex_replace(
        result,
        item_field_access_from_typed,
        "$1$2->id$3$2->name$4$2->value$5"
    );

    const std::regex item_field_access_from_typed_vararg(
        R"((__mingw_printf\s*\(\s*"Item %d %s %.2f\\n"\s*,\s*)\(ulonglong\)\(uint\)\*\(uint \*\)([A-Za-z_][A-Za-z0-9_]*)(\s*,\s*)\(char \*\)\(\(longlong\)\2 \+ 4\)(\s*,\s*)\*\(double \*\)\(\(longlong\)\2 \+ 0x28\)(\s*\)))"
    );
    result = std::regex_replace(
        result,
        item_field_access_from_typed_vararg,
        "$1(ulonglong)$2->id$3$2->name$4$2->value$5"
    );

    return result;
}

std::string normalize_msvc_crt_printf(const std::string& code) {
    std::string result = code;

    size_t pos = 0;
    while (true) {
        size_t fn_start = result.find("__stdio_common_v", pos);
        if (fn_start == std::string::npos) break;

        size_t fn_end = fn_start + 16;
        while (fn_end < result.size() &&
               (std::isalnum(static_cast<unsigned char>(result[fn_end])) || result[fn_end] == '_'))
            fn_end++;

        std::string fn_name = result.substr(fn_start, fn_end - fn_start);
        if (fn_name.find("printf") == std::string::npos) {
            pos = fn_end;
            continue;
        }

        size_t paren_start = fn_end;
        while (paren_start < result.size() && result[paren_start] == ' ') paren_start++;
        if (paren_start >= result.size() || result[paren_start] != '(') {
            pos = fn_end;
            continue;
        }

        int depth = 1;
        size_t p = paren_start + 1;
        std::vector<std::string> args;
        std::string cur;
        bool in_str = false;
        char str_delim = 0;

        while (p < result.size() && depth > 0) {
            char c = result[p];
            if (in_str) {
                cur += c;
                if (c == '\\') {
                    p++;
                    if (p < result.size()) cur += result[p];
                } else if (c == str_delim) {
                    in_str = false;
                }
            } else if (c == '"' || c == '\'') {
                in_str = true;
                str_delim = c;
                cur += c;
            } else if (c == '(') {
                depth++;
                cur += c;
            } else if (c == ')') {
                depth--;
                if (depth == 0) {
                    args.push_back(cur);
                    cur.clear();
                } else {
                    cur += c;
                }
            } else if (c == ',' && depth == 1) {
                args.push_back(cur);
                cur.clear();
            } else {
                cur += c;
            }
            p++;
        }

        size_t call_end = p;

        if (args.size() < 3) {
            pos = fn_end;
            continue;
        }

        auto trim = [](const std::string& s) -> std::string {
            size_t a = s.find_first_not_of(" \t\n\r");
            if (a == std::string::npos) return "";
            size_t b = s.find_last_not_of(" \t\n\r");
            return s.substr(a, b - a + 1);
        };

        std::string format = trim(args[2]);

        std::string new_call = "printf(" + format;
        for (size_t i = 4; i < args.size(); i++) {
            std::string arg = trim(args[i]);
            if (!arg.empty() && arg[0] == '&') {
                arg = arg.substr(1);
                arg = trim(arg);
            }
            new_call += ", " + arg;
        }
        new_call += ")";

        result.replace(fn_start, call_end - fn_start, new_call);
        pos = fn_start + new_call.size();
    }

    {
        std::istringstream iss(result);
        std::string line;
        std::string filtered;
        filtered.reserve(result.size());
        while (std::getline(iss, line)) {
            bool skip = (line.find("__acrt_iob_func") != std::string::npos ||
                         line.find("__local_stdio_printf_options") != std::string::npos);
            if (!skip) {
                filtered += line;
                filtered += '\n';
            }
        }
        result = filtered;
    }

    {
        auto count_word = [](const std::string& text, const std::string& word) -> int {
            int cnt = 0;
            size_t wlen = word.size();
            size_t pos2 = 0;
            while ((pos2 = text.find(word, pos2)) != std::string::npos) {
                bool left_ok  = (pos2 == 0 ||
                    (!std::isalnum(static_cast<unsigned char>(text[pos2 - 1])) &&
                      text[pos2 - 1] != '_'));
                bool right_ok = (pos2 + wlen >= text.size() ||
                    (!std::isalnum(static_cast<unsigned char>(text[pos2 + wlen])) &&
                      text[pos2 + wlen] != '_'));
                if (left_ok && right_ok) cnt++;
                pos2++;
            }
            return cnt;
        };

        auto parse_decl_varname = [](const std::string& ln) -> std::string {
            size_t e = ln.find_last_not_of(" \t\n\r");
            if (e == std::string::npos || ln[e] != ';') return "";
            if (ln.find('=') != std::string::npos) return "";
            if (ln.find('(') != std::string::npos) return "";
            {
                size_t first = ln.find_first_not_of(" \t");
                if (first != std::string::npos && ln.substr(first, 6) == "return") return "";
            }
            size_t nameEnd = e - 1;
            while (nameEnd > 0 && std::isspace(static_cast<unsigned char>(ln[nameEnd]))) nameEnd--;
            if (!std::isalnum(static_cast<unsigned char>(ln[nameEnd])) && ln[nameEnd] != '_')
                return "";
            size_t nameStart = nameEnd;
            while (nameStart > 0 &&
                   (std::isalnum(static_cast<unsigned char>(ln[nameStart - 1])) ||
                    ln[nameStart - 1] == '_'))
                nameStart--;
            std::string vname = ln.substr(nameStart, nameEnd - nameStart + 1);
            if (!vname.empty() && std::isdigit(static_cast<unsigned char>(vname[0]))) return "";
            static const std::set<std::string> kws = {
                "return","if","else","while","for","do","switch","case","break",
                "continue","goto","typedef","struct","union","enum","void"
            };
            if (kws.count(vname)) return "";
            return vname;
        };

        auto parse_assign_lhs = [](const std::string& ln) -> std::string {
            size_t e = ln.find_last_not_of(" \t\n\r");
            if (e == std::string::npos || ln[e] != ';') return "";
            if (ln.find('(') != std::string::npos) return "";
            {
                size_t first = ln.find_first_not_of(" \t");
                if (first != std::string::npos && ln.substr(first, 6) == "return") return "";
            }
            size_t eq = ln.find('=');
            if (eq == std::string::npos || eq == 0) return "";
            char before = ln[eq - 1];
            if (before == '!' || before == '<' || before == '>' ||
                before == '=' || before == '+' || before == '-' ||
                before == '*' || before == '/' || before == '&' ||
                before == '|' || before == '^') return "";
            if (eq + 1 < ln.size() && ln[eq + 1] == '=') return "";
            size_t ne = eq - 1;
            while (ne > 0 && std::isspace(static_cast<unsigned char>(ln[ne]))) ne--;
            if (!std::isalnum(static_cast<unsigned char>(ln[ne])) && ln[ne] != '_') return "";
            size_t ns = ne;
            while (ns > 0 && (std::isalnum(static_cast<unsigned char>(ln[ns - 1])) ||
                               ln[ns - 1] == '_'))
                ns--;
            return ln.substr(ns, ne - ns + 1);
        };

        bool changed = true;
        while (changed) {
            changed = false;

            std::vector<std::string> lns;
            {
                std::istringstream iss2(result);
                std::string ln;
                while (std::getline(iss2, ln)) lns.push_back(ln);
            }

            for (size_t i = 0; i < lns.size(); i++) {
                std::string vname = parse_decl_varname(lns[i]);
                if (vname.empty()) continue;
                std::string other;
                other.reserve(result.size());
                for (size_t j = 0; j < lns.size(); j++) {
                    if (j != i) { other += lns[j]; other += '\n'; }
                }
                if (count_word(other, vname) == 0) {
                    lns.erase(lns.begin() + i);
                    changed = true;
                    break;
                }
            }
            if (changed) {
                std::string rebuilt;
                for (const auto& ln : lns) { rebuilt += ln; rebuilt += '\n'; }
                result = rebuilt;
                continue;
            }

            for (size_t i = 0; i < lns.size(); i++) {
                std::string vname = parse_assign_lhs(lns[i]);
                if (vname.empty()) continue;
                std::string other;
                other.reserve(result.size());
                for (size_t j = 0; j < lns.size(); j++) {
                    if (j != i) { other += lns[j]; other += '\n'; }
                }
                int cnt = count_word(other, vname);
                if (cnt <= 1) {
                    lns.erase(lns.begin() + i);
                    changed = true;
                    break;
                }
            }
            if (changed) {
                std::string rebuilt;
                for (const auto& ln : lns) { rebuilt += ln; rebuilt += '\n'; }
                result = rebuilt;
            }
        }
    }

    return result;
}

} // namespace processing
} // namespace fission
