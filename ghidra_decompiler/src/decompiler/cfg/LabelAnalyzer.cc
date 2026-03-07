/**
 * LabelAnalyzer Implementation
 * 
 * Utilities for finding and analyzing labels and goto statements in C code.
 * Provides helper functions for determining control flow structures.
 */

#include "fission/decompiler/cfg/LabelAnalyzer.h"
#include <regex>
#include <algorithm>
#include <set>

namespace fission {
namespace decompiler {
namespace cfg {

std::string LabelAnalyzer::negate_condition(const std::string& condition) {
    std::string cond = condition;
    // Trim whitespace
    size_t start = cond.find_first_not_of(" \t\n");
    size_t end = cond.find_last_not_of(" \t\n");
    if (start != std::string::npos && end != std::string::npos) {
        cond = cond.substr(start, end - start + 1);
    }
    
    if (cond.empty()) return "true";
    
    // Check for already negated
    if (cond[0] == '!' && cond.size() > 1) {
        if (cond[1] == '(') {
            // Find matching paren
            int depth = 1;
            size_t i = 2;
            for (; i < cond.size() && depth > 0; i++) {
                if (cond[i] == '(') depth++;
                else if (cond[i] == ')') depth--;
            }
            if (depth == 0 && i == cond.size()) {
                return cond.substr(2, cond.size() - 3);
            }
        } else {
            return cond.substr(1);
        }
    }
    
    // Handle comparison operators — simple string replacement, no regex needed
    // Replace the FIRST occurrence only (operator appears exactly once in simple conditions)
    auto str_replace1 = [](std::string s, const std::string& from, const std::string& to) -> std::string {
        size_t p = s.find(from);
        if (p != std::string::npos) s.replace(p, from.size(), to);
        return s;
    };
    if (cond.find("==") != std::string::npos) {
        return str_replace1(cond, "==", "!=");
    } else if (cond.find("!=") != std::string::npos) {
        return str_replace1(cond, "!=", "==");
    } else if (cond.find(">=") != std::string::npos) {
        return str_replace1(cond, ">=", "<");
    } else if (cond.find("<=") != std::string::npos) {
        return str_replace1(cond, "<=", ">");
    } else if (cond.find(">") != std::string::npos) {
        return str_replace1(cond, ">", "<=");
    } else if (cond.find("<") != std::string::npos) {
        return str_replace1(cond, "<", ">=");
    }
    
    return "!(" + cond + ")";
}

std::vector<size_t> LabelAnalyzer::build_newline_index(const std::string& s) {
    std::vector<size_t> idx;
    for (size_t i = 0; i < s.size(); ++i)
        if (s[i] == '\n') idx.push_back(i);
    return idx;
}

int LabelAnalyzer::pos_to_line(const std::vector<size_t>& nl_idx, size_t pos) {
    // upper_bound gives the number of newlines strictly before `pos`
    return static_cast<int>(
        std::upper_bound(nl_idx.begin(), nl_idx.end(), pos) - nl_idx.begin()) + 1;
}

std::vector<LabelAnalyzer::Label> LabelAnalyzer::find_labels(const std::string& c_code) {
    std::vector<Label> labels;
    static const std::regex label_pattern(R"((?:^|\n)\s*((?!case\b|default\b)[A-Za-z_]\w*)\s*:(?!\s*:))");

    const auto nl_idx = build_newline_index(c_code);
    std::string::const_iterator search_start = c_code.cbegin();
    std::smatch match;

    while (std::regex_search(search_start, c_code.cend(), match, label_pattern)) {
        size_t pos = match.position() + (search_start - c_code.cbegin());
        int line = pos_to_line(nl_idx, pos);

        Label label;
        label.name = match[1].str();
        label.line = line;
        label.is_loop_target = false;
        label.is_used = false;
        labels.push_back(label);

        search_start = match.suffix().first;
    }

    return labels;
}

std::vector<LabelAnalyzer::GotoInfo> LabelAnalyzer::find_gotos(const std::string& c_code) {
    std::vector<GotoInfo> gotos;

    // Pattern for conditional goto: if (cond) goto label;
    static const std::regex cond_goto_pattern(R"(if\s*\(([^)]+)\)\s*goto\s+(\w+)\s*;)");
    // Pattern for unconditional goto: goto label;
    static const std::regex uncond_goto_pattern(R"(\bgoto\s+(\w+)\s*;)");

    const auto nl_idx = build_newline_index(c_code);
    std::string::const_iterator search_start = c_code.cbegin();
    std::smatch match;

    // Find conditional gotos first
    while (std::regex_search(search_start, c_code.cend(), match, cond_goto_pattern)) {
        size_t pos = match.position() + (search_start - c_code.cbegin());

        GotoInfo info;
        info.condition = match[1].str();
        info.target_label = match[2].str();
        info.line = pos_to_line(nl_idx, pos);
        info.is_forward = true;
        gotos.push_back(info);

        search_start = match.suffix().first;
    }

    // Find unconditional gotos
    search_start = c_code.cbegin();
    while (std::regex_search(search_start, c_code.cend(), match, uncond_goto_pattern)) {
        // Check if this is part of a conditional goto by looking at preceding text
        size_t match_pos = match.position() + (search_start - c_code.cbegin());
        std::string before = c_code.substr(std::max((size_t)0, match_pos > 50 ? match_pos - 50 : 0),
                                           std::min((size_t)50, match_pos));
        if (before.rfind(")") != std::string::npos &&
            before.rfind(")") > before.rfind(";") &&
            before.rfind("if") != std::string::npos) {
            search_start = match.suffix().first;
            continue;
        }

        GotoInfo info;
        info.condition = "";
        info.target_label = match[1].str();
        info.line = pos_to_line(nl_idx, match_pos);
        info.is_forward = true;
        gotos.push_back(info);

        search_start = match.suffix().first;
    }

    return gotos;
}

bool LabelAnalyzer::is_loop_header(const std::string& label,
                                    const std::vector<GotoInfo>& gotos,
                                    const std::vector<Label>& labels) {
    int label_line = -1;
    for (const auto& l : labels) {
        if (l.name == label) {
            label_line = l.line;
            break;
        }
    }
    
    if (label_line == -1) return false;
    
    for (const auto& g : gotos) {
        if (g.target_label == label && g.line > label_line) {
            return true;
        }
    }
    
    return false;
}

std::string LabelAnalyzer::remove_unused_labels(const std::string& c_code) {
    // Early exit: if no labels at all, nothing to do.
    if (c_code.find(':') == std::string::npos) return c_code;

    std::vector<Label>    labels = find_labels(c_code);
    if (labels.empty()) return c_code;

    std::vector<GotoInfo> gotos  = find_gotos(c_code);

    std::set<std::string> used_labels;
    for (const auto& g : gotos) used_labels.insert(g.target_label);

    // Build set of unused label names
    std::set<std::string> unused;
    for (const auto& label : labels)
        if (used_labels.find(label.name) == used_labels.end())
            unused.insert(label.name);

    if (unused.empty()) return c_code;

    // Manual line-by-line scan: drop lines that are purely an unused label.
    // A label line looks like: (optional whitespace) NAME : (optional whitespace)
    // We do NOT use per-label regex to avoid O(L) compilations.
    bool ends_nl = !c_code.empty() && c_code.back() == '\n';
    std::string out;
    out.reserve(c_code.size());

    size_t i = 0;
    while (i < c_code.size()) {
        size_t line_end = c_code.find('\n', i);
        bool has_newline = (line_end != std::string::npos);
        size_t line_len  = has_newline ? line_end - i : c_code.size() - i;

        // Trim leading whitespace to check if line is "LABEL:"
        size_t s = i;
        while (s < i + line_len && (c_code[s] == ' ' || c_code[s] == '\t')) s++;
        // scan identifier
        size_t id_start = s;
        while (s < i + line_len && (std::isalnum(static_cast<unsigned char>(c_code[s])) || c_code[s] == '_')) s++;
        size_t id_end = s;
        // skip whitespace
        while (s < i + line_len && (c_code[s] == ' ' || c_code[s] == '\t')) s++;

        bool drop = false;
        if (id_end > id_start && s < i + line_len && c_code[s] == ':') {
            // Check nothing else meaningful on this line after the colon
            size_t after = s + 1;
            while (after < i + line_len && (c_code[after] == ' ' || c_code[after] == '\t')) after++;
            if (after == i + line_len) {
                // Pure label line
                std::string name(c_code, id_start, id_end - id_start);
                if (unused.count(name)) drop = true;
            }
        }

        if (!drop) {
            out.append(c_code, i, line_len);
            if (has_newline) out.push_back('\n');
        }

        i = has_newline ? line_end + 1 : c_code.size();
    }

    if (ends_nl && !out.empty() && out.back() != '\n') out.push_back('\n');
    return out;
}

} // namespace cfg
} // namespace decompiler
} // namespace fission
