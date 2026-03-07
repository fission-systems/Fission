#include <cstring>
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
        // Only consider constants with >= 8 hex chars (>= 4 raw bytes)
        if (len >= 8) {
            // ── Arithmetic-context guard ───────────────────────────────────
            // Skip conversion if the constant appears directly adjacent to an
            // arithmetic / bitwise operator — it's almost certainly a numeric
            // operand, not a packed string literal.
            bool in_arithmetic = false;
            if (start >= 1) {
                size_t prev = start - 1;
                while (prev > 0 && (c_code[prev] == ' ' || c_code[prev] == '\t'))
                    prev--;
                char pc = c_code[prev];
                if (pc == '+' || pc == '-' || pc == '*' || pc == '/' ||
                    pc == '&' || pc == '|' || pc == '^' || pc == '~' ||
                    pc == '%' || pc == '<' || pc == '>') {
                    in_arithmetic = true;
                }
            }
            if (!in_arithmetic && end < c_code.length()) {
                char nc = c_code[end];
                if (nc == '+' || nc == '-' || nc == '*' || nc == '/' ||
                    nc == '&' || nc == '|' || nc == '^' || nc == '%') {
                    in_arithmetic = true;
                }
            }

            if (!in_arithmetic) {
                std::string hex_str = c_code.substr(start, len);
                try {
                    unsigned long long val = std::stoull(hex_str, nullptr, 16);

                    // ── High-byte zero guard ──────────────────────────────
                    // A zero most-significant byte means the constant is
                    // likely a padded numeric value, not a packed string.
                    int hex_digits = static_cast<int>(len) - 2; // subtract "0x"
                    int num_bytes  = (hex_digits + 1) / 2;
                    if (num_bytes >= 2) {
                        unsigned long long high_mask = 0xFFULL << ((num_bytes - 1) * 8);
                        if ((val & high_mask) == 0) {
                            pos = end;
                            continue; // Zero high byte — numeric constant, skip
                        }
                    }

                    // ── Extract bytes (Little Endian for x86) ────────────
                    std::string decoded;
                    bool is_ascii = true;
                    unsigned long long temp = val;

                    std::vector<char> bytes;
                    while (temp > 0) {
                        char c = (char)(temp & 0xFF);
                        bytes.push_back(c);
                        temp >>= 8;
                    }

                    if (bytes.empty()) is_ascii = false;

                    int valid_chars = 0;
                    for (char c : bytes) {
                        if (c == 0) continue; // Allow null terminators
                        // Require the byte to be a printable ASCII character
                        if (c >= 0x20 && c <= 0x7E &&
                            (isalnum((unsigned char)c) || ispunct((unsigned char)c) || c == ' ')) {
                            valid_chars++;
                            decoded += c;
                        } else {
                            is_ascii = false;
                            break;
                        }
                    }

                    // Require >= 4 consecutive printable chars (was 3)
                    if (is_ascii && valid_chars >= 4) {
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
    // ── 1. Double-paren: ((expr)) → (expr) ─────────────────────────────────
    static const std::regex double_paren(R"(\(\(([^()]+)\)\))");
    c_code = std::regex_replace(c_code, double_paren, "($1)");

    // ── 2. if (x != 0) → if (x)  /  if (x == 0) → if (!x) ──────────────
    static const std::regex non_zero_check(R"(if\s*\(\s*(\w+)\s*!=\s*0\s*\))");
    static const std::regex zero_check(R"(if\s*\(\s*(\w+)\s*==\s*0\s*\))");
    c_code = std::regex_replace(c_code, non_zero_check, "if ($1)");
    c_code = std::regex_replace(c_code, zero_check, "if (!$1)");

    // ── 3. if ((cast)x != 0) → if (x)  /  if ((cast)x == 0) → if (!x) ─────
    // Covers: (int), (uint), (longlong), (ulonglong), (short), (ushort), (byte)
    static const std::regex cast_nonzero(
        R"(if\s*\(\s*\(\s*(?:u?longlong|u?int|u?short|byte)\s*\)\s*(\w+)\s*!=\s*0\s*\))");
    static const std::regex cast_zero(
        R"(if\s*\(\s*\(\s*(?:u?longlong|u?int|u?short|byte)\s*\)\s*(\w+)\s*==\s*0\s*\))");
    c_code = std::regex_replace(c_code, cast_nonzero, "if ($1)");
    c_code = std::regex_replace(c_code, cast_zero, "if (!$1)");

    // ── 4. !!x → (bool)x  (double logical-not is display noise) ──────────
    // Only safe when x is a single word (no side-effect concern).
    static const std::regex double_not(R"(!!\s*(\w+))");
    c_code = std::regex_replace(c_code, double_not, "(bool)$1");

    // ── 5. while ((cast)x != 0) → while (x)  /  while ((cast)x == 0) → while (!x) ─
    static const std::regex while_cast_nonzero(
        R"(while\s*\(\s*\(\s*(?:u?longlong|u?int|u?short|byte)\s*\)\s*(\w+)\s*!=\s*0\s*\))");
    static const std::regex while_cast_zero(
        R"(while\s*\(\s*\(\s*(?:u?longlong|u?int|u?short|byte)\s*\)\s*(\w+)\s*==\s*0\s*\))");
    c_code = std::regex_replace(c_code, while_cast_nonzero, "while ($1)");
    c_code = std::regex_replace(c_code, while_cast_zero, "while (!$1)");

    return c_code;
}

// ─────────────────────────────────────────────────────────────────────────────
// rewrite_pointer_arithmetic_to_array
// ─────────────────────────────────────────────────────────────────────────────
// Converts common Ghidra pointer-dereference patterns to readable array
// subscript notation.  Run BEFORE eliminate_redundant_casts so that any
// (longlong)/(char *) casts left after substitution are cleaned up there.
//
// Pattern A: *(T *)((char *)VAR + N)                 →  ((T *)VAR)[N/sz]
// Pattern B: *(T *)((longlong)VAR + (longlong)IDX * sz)  →  VAR[IDX]
// Pattern C: *(T *)(VAR + IDX * sz)                  →  VAR[IDX]
// Pattern D: *(T *)((longlong)VAR + N)               →  ((T *)VAR)[N/sz]
//
// Only fires when the constant factor equals sizeof(T) (strong validation).
// ─────────────────────────────────────────────────────────────────────────────
std::string PostProcessor::rewrite_pointer_arithmetic_to_array(std::string c_code) {
    // Type → byte-width table (every type emitted by Ghidra + our normaliser).
    static const struct { const char* nm; int sz; } type_sizes[] = {
        {"uint64_t",  8}, {"int64_t",   8}, {"ulonglong", 8}, {"longlong",  8},
        {"double",    8}, {"uint32_t",  4}, {"int32_t",   4}, {"uint",      4},
        {"int",       4}, {"float",     4}, {"uint16_t",  2}, {"int16_t",   2},
        {"ushort",    2}, {"short",     2}, {"uint8_t",   1}, {"int8_t",    1},
        {"byte",      1}, {"char",      1},
    };
    auto get_size = [&](const std::string& t) -> int {
        for (auto& p : type_sizes)
            if (t == p.nm) return p.sz;
        return 0; // unknown type — don't transform
    };

    // ── Pass A: *(T *)((char *)VAR + N) → ((T *)VAR)[N/sz] ─────────────────
    {
        static const std::regex patA(
            R"(\*\(\s*(\w+)\s*\*\s*\)\s*\(\s*\(\s*char\s*\*\s*\)\s*(\w+)\s*\+\s*(\d+)\s*\))"
        );
        std::string out;
        out.reserve(c_code.size());
        size_t last = 0;
        bool changed = false;
        for (auto it = std::sregex_iterator(c_code.begin(), c_code.end(), patA);
             it != std::sregex_iterator(); ++it) {
            const auto& m = *it;
            int offset = std::stoi(m[3].str());
            int sz = get_size(m[1].str());
            if (sz <= 0 || offset % sz != 0) continue;
            int idx = offset / sz;
            out += c_code.substr(last, m.position() - last);
            if (sz == 1)
                out += m[2].str() + "[" + std::to_string(idx) + "]";
            else
                out += "((" + m[1].str() + " *)" + m[2].str() + ")[" + std::to_string(idx) + "]";
            last = m.position() + m.length();
            changed = true;
        }
        if (changed) { out += c_code.substr(last); c_code = std::move(out); }
    }

    // ── Pass B: *(T *)((longlong)VAR + (longlong)IDX * SZ) → VAR[IDX] ───────
    // Both operands carry (longlong)/(u?int64_t) casts — typical on x86-64.
    {
        static const std::regex patB(
            R"(\*\(\s*(\w+)\s*\*\s*\)\s*\(\s*\(\s*(?:u?longlong|u?int64_t)\s*\)\s*(\w+)\s*\+\s*\(\s*(?:u?longlong|u?int64_t)\s*\)\s*(\w+)\s*\*\s*(\d+)\s*\))"
        );
        std::string out;
        out.reserve(c_code.size());
        size_t last = 0;
        bool changed = false;
        for (auto it = std::sregex_iterator(c_code.begin(), c_code.end(), patB);
             it != std::sregex_iterator(); ++it) {
            const auto& m = *it;
            int elem_sz = std::stoi(m[4].str());
            int sz = get_size(m[1].str());
            if (sz <= 0 || sz != elem_sz) continue;
            out += c_code.substr(last, m.position() - last);
            out += m[2].str() + "[" + m[3].str() + "]";
            last = m.position() + m.length();
            changed = true;
        }
        if (changed) { out += c_code.substr(last); c_code = std::move(out); }
    }

    // ── Pass C: *(T *)(VAR + IDX * SZ) → VAR[IDX] ──────────────────────────
    // No casts — occurs in 32-bit code or after prior cast-elimination.
    {
        static const std::regex patC(
            R"(\*\(\s*(\w+)\s*\*\s*\)\s*\(\s*(\w+)\s*\+\s*(\w+)\s*\*\s*(\d+)\s*\))"
        );
        std::string out;
        out.reserve(c_code.size());
        size_t last = 0;
        bool changed = false;
        for (auto it = std::sregex_iterator(c_code.begin(), c_code.end(), patC);
             it != std::sregex_iterator(); ++it) {
            const auto& m = *it;
            int elem_sz = std::stoi(m[4].str());
            int sz = get_size(m[1].str());
            if (sz <= 0 || sz != elem_sz) continue;
            out += c_code.substr(last, m.position() - last);
            out += m[2].str() + "[" + m[3].str() + "]";
            last = m.position() + m.length();
            changed = true;
        }
        if (changed) { out += c_code.substr(last); c_code = std::move(out); }
    }

    // ── Pass D: *(T *)((longlong)VAR + N) → ((T *)VAR)[N/sz] ───────────────
    // Only base has cast; second operand is a literal byte offset.
    {
        static const std::regex patD(
            R"(\*\(\s*(\w+)\s*\*\s*\)\s*\(\s*\(\s*(?:u?longlong|u?int64_t)\s*\)\s*(\w+)\s*\+\s*(\d+)\s*\))"
        );
        std::string out;
        out.reserve(c_code.size());
        size_t last = 0;
        bool changed = false;
        for (auto it = std::sregex_iterator(c_code.begin(), c_code.end(), patD);
             it != std::sregex_iterator(); ++it) {
            const auto& m = *it;
            int offset = std::stoi(m[3].str());
            int sz = get_size(m[1].str());
            if (sz <= 0 || offset % sz != 0) continue;
            int idx = offset / sz;
            out += c_code.substr(last, m.position() - last);
            if (sz == 1)
                out += m[2].str() + "[" + std::to_string(idx) + "]";
            else
                out += "((" + m[1].str() + " *)" + m[2].str() + ")[" + std::to_string(idx) + "]";
            last = m.position() + m.length();
            changed = true;
        }
        if (changed) { out += c_code.substr(last); c_code = std::move(out); }
    }

    return c_code;
}

// ─────────────────────────────────────────────────────────────────────────────
// eliminate_redundant_casts
// ─────────────────────────────────────────────────────────────────────────────
// Removes display-noise casts that Ghidra emits:
//   • widening wrappers: (ulonglong)(uint)x    → (uint)x
//   • same-type double:  (int)(int)x           → (int)x
//   • null pointer:      (void*)0 / (void *)0x0 → NULL
// ─────────────────────────────────────────────────────────────────────────────
std::string PostProcessor::eliminate_redundant_casts(std::string c_code) {
    // 1. (void*)0 / (void *)0x0... → NULL
    static const std::regex void_null(R"(\(\s*void\s*\*\s*\)\s*0x?0*\b)");
    c_code = std::regex_replace(c_code, void_null, "NULL");

    // 2. Widening wrapper removal: outer widens inner — outer is noise
    static const std::regex widen_ull_uint(R"(\(ulonglong\)\s*(\(uint\)))");
    static const std::regex widen_ull_ushort(R"(\(ulonglong\)\s*(\(ushort\)))");
    static const std::regex widen_ull_byte(R"(\(ulonglong\)\s*(\(byte\)))");
    static const std::regex widen_ll_int(R"(\(longlong\)\s*(\(int\)))");
    static const std::regex widen_ll_short(R"(\(longlong\)\s*(\(short\)))");
    c_code = std::regex_replace(c_code, widen_ull_uint, "$1");
    c_code = std::regex_replace(c_code, widen_ull_ushort, "$1");
    c_code = std::regex_replace(c_code, widen_ull_byte, "$1");
    c_code = std::regex_replace(c_code, widen_ll_int, "$1");
    c_code = std::regex_replace(c_code, widen_ll_short, "$1");

    // 3. Same-type double cast: (T)(T) → (T)
    static const std::regex same_int(R"(\(int\)\s*\(int\))");
    static const std::regex same_uint(R"(\(uint\)\s*\(uint\))");
    static const std::regex same_ll(R"(\(longlong\)\s*\(longlong\))");
    static const std::regex same_ull(R"(\(ulonglong\)\s*\(ulonglong\))");
    static const std::regex same_short(R"(\(short\)\s*\(short\))");
    static const std::regex same_ushort(R"(\(ushort\)\s*\(ushort\))");
    static const std::regex same_byte(R"(\(byte\)\s*\(byte\))");
    c_code = std::regex_replace(c_code, same_int, "(int)");
    c_code = std::regex_replace(c_code, same_uint, "(uint)");
    c_code = std::regex_replace(c_code, same_ll, "(longlong)");
    c_code = std::regex_replace(c_code, same_ull, "(ulonglong)");
    c_code = std::regex_replace(c_code, same_short, "(short)");
    c_code = std::regex_replace(c_code, same_ushort, "(ushort)");
    c_code = std::regex_replace(c_code, same_byte, "(byte)");

    return c_code;
}

// ─────────────────────────────────────────────────────────────────────────────
// eliminate_dead_stores
// ─────────────────────────────────────────────────────────────────────────────
// Removes trivially-dead / no-op assignments:
//   • Self-assignment:  x = x;  /  local_8 = local_8;
// Full dead-store elimination (liveness analysis) is not done here; that
// requires a proper data-flow pass operated on the Pcode IR.
// ─────────────────────────────────────────────────────────────────────────────
std::string PostProcessor::eliminate_dead_stores(std::string c_code) {
    bool ends_nl = !c_code.empty() && c_code.back() == '\n';
    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    // Pattern: optional-indent  IDENTIFIER = IDENTIFIER ;
    // (only simple word identifiers — struct field self-assigns are left alone
    //  to avoid accidentally breaking intentional memory-barrier stores)
    static const std::regex self_assign(R"(^(\s*)(\w+)\s*=\s*\2\s*;\s*$)");

    bool changed = false;
    std::vector<std::string> newlines;
    newlines.reserve(lines.size());
    for (const auto& ln : lines) {
        if (std::regex_match(ln, self_assign)) {
            changed = true;
            continue; // drop the line
        }
        newlines.push_back(ln);
    }

    if (!changed) return c_code;

    std::string out;
    out.reserve(c_code.size());
    for (size_t i = 0; i < newlines.size(); i++) {
        out += newlines[i];
        if (i + 1 < newlines.size()) out += '\n';
    }
    if (ends_nl && (out.empty() || out.back() != '\n')) out += '\n';
    return out;
}

std::string PostProcessor::fold_array_init(std::string c_code) {
    // Detect runs of consecutive: var[0] = v0; var[1] = v1; ...
    // and fold them into a single initializer: var[] = {v0, v1, ...};
    // Only triggers when the run starts at index 0 and has >= 3 elements.
    static const std::regex arr_assign(
        R"(^(\s*)(\w+)\[(\d+)\]\s*=\s*([^;]+);\s*$)"
    );

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
            std::smatch m;
            if (!std::regex_match(lines[i], m, arr_assign)) continue;
            std::string indent = m[1].str();
            std::string var    = m[2].str();
            int idx0 = std::stoi(m[3].str());
            if (idx0 != 0) continue; // only handle runs starting at [0]

            std::vector<std::string> values;
            {
                std::string v = m[4].str();
                while (!v.empty() && std::isspace((unsigned char)v.back())) v.pop_back();
                values.push_back(v);
            }

            // Collect consecutive ascending indices on the same variable
            size_t j = i + 1;
            for (; j < lines.size(); j++) {
                std::smatch m2;
                if (!std::regex_match(lines[j], m2, arr_assign)) break;
                if (m2[2].str() != var) break;
                if (std::stoi(m2[3].str()) != static_cast<int>(values.size())) break;
                std::string v = m2[4].str();
                while (!v.empty() && std::isspace((unsigned char)v.back())) v.pop_back();
                values.push_back(v);
            }

            // Only fold if we have >= 3 consecutive assignments
            if (values.size() < 3) continue;

            // Build initializer list (omit explicit size — let compiler infer)
            std::ostringstream init;
            init << indent << var << "[] = {";
            for (size_t k = 0; k < values.size(); k++) {
                if (k > 0) init << ", ";
                init << values[k];
            }
            init << "};";

            std::vector<std::string> newlines;
            for (size_t k = 0; k < i; k++) newlines.push_back(lines[k]);
            newlines.push_back(init.str());
            for (size_t k = j; k < lines.size(); k++) newlines.push_back(lines[k]);
            lines = std::move(newlines);
            any_changed = true;
            break;
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

std::string PostProcessor::improve_variable_names(std::string c_code) {
    // ── 1. Return variable → "result" ─────────────────────────────────────
    {
        static const std::regex return_var_pattern(R"(return\s+(local_\w+)\s*;)");
        std::smatch match;
        if (std::regex_search(c_code, match, return_var_pattern)) {
            std::string var_name = match[1].str();
            if (var_name.rfind("local_", 0) == 0) {
                const std::regex var_pattern("\\b" + var_name + "\\b");
                auto it  = std::sregex_iterator(c_code.begin(), c_code.end(), var_pattern);
                int count = static_cast<int>(std::distance(it, std::sregex_iterator()));
                if (count >= 2 && count <= 10) {
                    c_code = std::regex_replace(c_code, var_pattern, "result");
                }
            }
        }
    }

    // ── 2. Allocator return → "buf" ────────────────────────────────────────
    {
        static const std::regex malloc_assign(
            R"(\b(local_\w+)\s*=\s*(?:malloc|calloc|realloc|HeapAlloc|VirtualAlloc)\s*\()"
        );
        std::sregex_iterator it(c_code.begin(), c_code.end(), malloc_assign);
        std::sregex_iterator end;
        for (; it != end; ++it) {
            std::string lv = (*it)[1].str();
            std::regex lv_pat("\\b" + lv + "\\b");
            auto it2 = std::sregex_iterator(c_code.begin(), c_code.end(), lv_pat);
            int cnt = static_cast<int>(std::distance(it2, std::sregex_iterator()));
            if (cnt >= 2 && cnt <= 15) {
                c_code = std::regex_replace(c_code, lv_pat, "buf");
                break; // only rename one allocator result per function
            }
        }
    }

    // ── 3. strlen/wcslen result → "len" ───────────────────────────────────
    {
        static const std::regex strlen_assign(
            R"(\b(local_\w+)\s*=\s*(?:strlen|wcslen|strnlen)\s*\()"
        );
        std::sregex_iterator it(c_code.begin(), c_code.end(), strlen_assign);
        std::sregex_iterator end;
        for (; it != end; ++it) {
            std::string lv = (*it)[1].str();
            std::regex lv_pat("\\b" + lv + "\\b");
            auto it2 = std::sregex_iterator(c_code.begin(), c_code.end(), lv_pat);
            int cnt = static_cast<int>(std::distance(it2, std::sregex_iterator()));
            if (cnt >= 2 && cnt <= 12) {
                c_code = std::regex_replace(c_code, lv_pat, "len");
                break;
            }
        }
    }

    // ── 4. for-loop counter starting at 0 → i / j / k / n ────────────────
    {
        static const std::regex for_counter(
            R"(\bfor\s*\(\s*(local_\w+)\s*=\s*0\s*;)"
        );
        static const char* names[] = {"i", "j", "k", "n"};
        int name_idx = 0;
        std::string cur = c_code;
        // Iterating over a copy; collect matches first to avoid invalidation.
        std::vector<std::string> loop_vars;
        {
            std::sregex_iterator it(cur.begin(), cur.end(), for_counter);
            std::sregex_iterator end;
            for (; it != end && name_idx < 4; ++it, ++name_idx)
                loop_vars.push_back((*it)[1].str());
        }
        for (int k = 0; k < static_cast<int>(loop_vars.size()); k++) {
            const std::string& lv = loop_vars[k];
            std::regex lv_pat("\\b" + lv + "\\b");
            auto it2 = std::sregex_iterator(cur.begin(), cur.end(), lv_pat);
            int cnt = static_cast<int>(std::distance(it2, std::sregex_iterator()));
            if (cnt >= 2 && cnt <= 20) {
                cur = std::regex_replace(cur, lv_pat, names[k]);
                // Update loop_vars[remaining] if same var was listed again
                for (int m = k + 1; m < static_cast<int>(loop_vars.size()); m++) {
                    if (loop_vars[m] == lv) loop_vars[m] = names[k];
                }
            }
        }
        c_code = cur;
    }

    return c_code;
}

std::string PostProcessor::structurize_control_flow(std::string c_code) {
    // Use CFGStructurizer for goto elimination and loop normalization
    return CFGStructurizer::structurize(c_code);
}

std::string PostProcessor::convert_while_to_for_struct(std::string c_code) {
    // Static regex objects — compiled once at first call (C++11 magic statics)
    // Extended to handle optional casts: while ((int)var < N) or while (var < N)
    static const std::regex while_pat(
        R"(^(\s*)while\s*\(\s*(?:\(\s*\w+\s*\)\s*)?(\w+)\s*(<=|<|>=|>|!=|==)\s*([^)]+)\)\s*\{\s*$)");
    // Also match cast in compound condition: while ((int)var op (int)expr)
    static const std::regex while_pat_cast(
        R"(^(\s*)while\s*\(\s*\(\s*\w+\s*\)\s*(\w+)\s*(<=|<|>=|>|!=|==)\s*([^)]+)\)\s*\{\s*$)");
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
            if (!std::regex_match(lines[i], wm, while_pat) &&
                !std::regex_match(lines[i], wm, while_pat_cast)) continue;

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

    // 7. Pointer arithmetic → array subscript ( *(T*)(ptr+i*sz) → ptr[i] )
    result = rewrite_pointer_arithmetic_to_array(result);

    // 8. Remove redundant / widening casts and replace (void*)0 with NULL
    result = eliminate_redundant_casts(result);

    // 9. Drop self-assignment lines (x = x;)
    result = eliminate_dead_stores(result);

    // 10. Improve variable names
    result = improve_variable_names(result);
    
    return result;
}

} // namespace decompiler
} // namespace fission

