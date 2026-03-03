// Copyright (C) 2024-2026 Fission Project
//
// This file is part of the Fission decompiler framework.
// It provides functionality for reconstructing switch statements from various
// control flow patterns commonly produced by compilers and decompilers.

#include "fission/decompiler/cfg/SwitchReconstructor.h"
#include "fission/utils/logger.h"

#include <regex>
#include <sstream>
#include <vector>
#include <algorithm>
#include <map>
#include <set>
#include <iostream>

namespace fission {
namespace decompiler {
namespace cfg {

std::string SwitchReconstructor::reconstruct_switch_from_jump_table(const std::string& c_code) {
    // Regex for a single equality-check branch:
    //   if (<var> == <val>) goto <label>;   OR
    //   if (<val> == <var>) goto <label>;
    // We capture: (var, val, target_label)  — normalising so var is the non-literal side.
    static const std::regex eq_goto(
        R"(^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    // Alternative: literal on left
    static const std::regex eq_goto_rev(
        R"(^(\s*)if\s*\(\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*==\s*(\w+)\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    // A label line:  WORD:
    static const std::regex label_line(R"(^(\s*)(\w+)\s*:[ \t]*$)");
    // A goto-break (goto to the switch exit label):
    static const std::regex goto_line(R"(^\s*goto\s+(\w+)\s*;[ \t]*$)");

    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    struct CaseEntry {
        std::string value;      // e.g. "0", "0x10"
        std::string label;      // target label name
        std::string indent;     // indent of the if-line
    };

    bool changed = false;

    // Scan for runs of consecutive equality-check lines.
    auto try_convert = [&](size_t start_idx) -> size_t {
        std::smatch m;
        std::string var_name, base_indent;
        std::vector<CaseEntry> cases;

        // Collect contiguous if-block
        size_t i = start_idx;
        for (; i < lines.size(); ++i) {
            const std::string& ln = lines[i];
            bool matched = false;
            std::string val, lbl, indent;
            // Handle cast on var: if ((int)var == N)  — strip cast for matching
            std::string ln_stripped = ln;
            // Try direct match
            if (std::regex_match(ln, m, eq_goto)) {
                indent   = m[1].str();
                std::string vn = m[2].str();
                val      = m[3].str();
                lbl      = m[4].str();
                if (cases.empty()) { var_name = vn; base_indent = indent; }
                if (vn == var_name) { cases.push_back({val, lbl, indent}); matched = true; }
            } else if (std::regex_match(ln, m, eq_goto_rev)) {
                indent   = m[1].str();
                val      = m[2].str();
                std::string vn = m[3].str();
                lbl      = m[4].str();
                if (cases.empty()) { var_name = vn; base_indent = indent; }
                if (vn == var_name) { cases.push_back({val, lbl, indent}); matched = true; }
            }
            if (!matched) break;
        }

        // Need at least 2 cases to be worth converting (single equality goto is
        // better handled by eliminate_forward_gotos).
        if (cases.size() < 2) return start_idx + 1;

        // i now points to the first non-case-check line.
        // Collect the "default" body: everything between end of if-chain and
        // the FIRST case label we recognise, followed by the exit goto.
        std::string exit_label;
        std::vector<std::string> default_lines;
        size_t j = i;
        {
            std::set<std::string> case_labels;
            for (const auto& c : cases) case_labels.insert(c.label);

            for (; j < lines.size(); ++j) {
                const std::string& ln = lines[j];
                // If we hit one of the case labels, default body ends.
                if (std::regex_match(ln, m, label_line)) {
                    if (case_labels.count(m[2].str())) break;
                }
                // Capture the exit-goto label if present before first case label.
                if (std::regex_match(ln, m, goto_line)) {
                    std::string tgt = m[1].str();
                    if (!case_labels.count(tgt)) {
                        // This is likely the exit goto — record and don't add to default body.
                        if (exit_label.empty()) exit_label = tgt;
                        continue;
                    }
                }
                default_lines.push_back(ln);
            }
        }

        // Build a map: label -> body lines (lines between "label:" and the next
        // recognised case-label or exit-label).
        std::set<std::string> case_labels_set;
        for (const auto& c : cases) case_labels_set.insert(c.label);

        std::map<std::string, std::vector<std::string>> bodies;
        std::string cur_label;
        for (size_t k = j; k < lines.size(); ++k) {
            const std::string& ln = lines[k];
            if (std::regex_match(ln, m, label_line)) {
                std::string lbl_name = m[2].str();
                if (case_labels_set.count(lbl_name)) {
                    cur_label = lbl_name;
                    continue;
                }
                // Exit label encountered
                if (!exit_label.empty() && lbl_name == exit_label) {
                    cur_label = "";
                    break;
                }
                // Some other label inside a body — include it.
            }
            if (!cur_label.empty()) {
                // Remove trailing "goto EXIT_LABEL;" line (it becomes break).
                if (std::regex_match(ln, m, goto_line)) {
                    std::string tgt = m[1].str();
                    if (!exit_label.empty() && tgt == exit_label) {
                        // This is a break — skip the goto, we'll emit break below.
                        continue;
                    }
                    // Check if this goto targets the NEXT case label in order.
                    // If so, this is a fallthrough — remove the goto and mark it.
                    // We'll handle break/fallthrough logic during switch emission.
                    if (case_labels_set.count(tgt)) {
                        // Record fallthrough: don't add goto to body, mark for no-break.
                        // We store a sentinel comment that the emitter checks.
                        bodies[cur_label].push_back("/* FALLTHROUGH */");
                        continue;
                    }
                }
                bodies[cur_label].push_back(ln);
            }
        }

        // Sanity check: every case must have a body (even empty).
        // If any case label has no body at all, the pattern was not matched
        // cleanly — bail out.
        for (const auto& c : cases) {
            if (!bodies.count(c.label) && !case_labels_set.count(c.label)) {
                return start_idx + 1; // Not a clean match
            }
        }

        // --- Build the switch statement ---
        std::ostringstream sw;
        sw << base_indent << "switch (" << var_name << ") {\n";

        for (const auto& ce : cases) {
            sw << base_indent << "case " << ce.value << ":\n";
            auto it = bodies.find(ce.label);
            if (it != bodies.end()) {
                for (const auto& bl : it->second) {
                    sw << bl << "\n";
                }
            }
            // Emit break unless last body line is return/goto/break or fallthrough.
            bool needs_break = true;
            auto& blines = bodies[ce.label];
            if (!blines.empty()) {
                const std::string& last = blines.back();
                if (last.find("return ") != std::string::npos ||
                    last.find("goto ")   != std::string::npos ||
                    last.find("break;")  != std::string::npos ||
                    last.find("/* FALLTHROUGH */") != std::string::npos) {
                    needs_break = false;
                }
            }
            if (needs_break) sw << base_indent << "  break;\n";
        }

        // Emit default: if there is a non-empty default body.
        bool has_default = false;
        for (const auto& dl : default_lines) {
            std::string t = dl;
            t.erase(0, t.find_first_not_of(" \t"));
            if (!t.empty() && t != "{" && t != "}") { has_default = true; break; }
        }
        if (has_default) {
            sw << base_indent << "default:\n";
            for (const auto& dl : default_lines) sw << dl << "\n";
        }

        sw << base_indent << "}";

        // Now figure out how many *original* lines the switch consumed.
        // It spans from start_idx to (and including) exit_label definition.
        // Find where exit_label: is defined after j.
        size_t end_idx = j; // j is where 1st case label is
        if (!exit_label.empty()) {
            for (size_t k = j; k < lines.size(); ++k) {
                if (std::regex_match(lines[k], m, label_line) && m[2].str() == exit_label) {
                    end_idx = k + 1; // include the exit label line itself
                    break;
                }
            }
        } else {
            // consume until after last case body
            end_idx = j;
            for (const auto& c : cases) {
                for (size_t k = j; k < lines.size(); ++k) {
                    if (std::regex_match(lines[k], m, label_line) && m[2].str() == c.label) {
                        auto& bdy = bodies[c.label];
                        // advance past body
                        size_t bl = k + 1 + bdy.size();
                        if (bl > end_idx) end_idx = bl;
                    }
                }
            }
        }

        // Replace lines[start_idx .. end_idx) with the switch text.
        std::vector<std::string> sw_lines;
        {
            std::istringstream ss(sw.str());
            std::string ln;
            while (std::getline(ss, ln)) sw_lines.push_back(ln);
        }

        lines.erase(lines.begin() + start_idx, lines.begin() + end_idx);
        lines.insert(lines.begin() + start_idx, sw_lines.begin(), sw_lines.end());

        changed = true;
        fission::utils::log_stream() << "[SwitchReconstructor] Reconstructed switch on '" << var_name
                  << "' with " << cases.size() << " cases" << std::endl;

        // Resume scanning after the newly inserted switch block.
        return start_idx + sw_lines.size();
    };

    size_t idx = 0;
    while (idx < lines.size()) {
        idx = try_convert(idx);
    }

    if (!changed) return c_code;

    std::ostringstream out;
    for (size_t i = 0; i < lines.size(); ++i) {
        out << lines[i];
        if (i + 1 < lines.size()) out << "\n";
    }
    return out.str();
}

// ============================================================================
// Switch Reconstruction from if-else-if chains
// ============================================================================
//
// Detects patterns like:
//   if (var == 0) {
//       body0;
//   } else if (var == 1) {
//       body1;
//   } else {
//       default_body;
//   }
//
// Reconstructed to:
//   switch (var) {
//   case 0: body0; break;
//   case 1: body1; break;
//   default: default_body;
//   }

std::string SwitchReconstructor::reconstruct_switch_from_if_else_chain(const std::string& c_code) {
    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    // Regex for: if (var == val) {
    static const std::regex if_eq_open(
        R"(^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*\)\s*\{)"
    );
    static const std::regex if_eq_open_rev(
        R"(^(\s*)if\s*\(\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*==\s*(\w+)\s*\)\s*\{)"
    );
    // Regex for: } else if (var == val) {
    static const std::regex else_if_eq(
        R"(^\s*\}\s*else\s+if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*\)\s*\{)"
    );
    static const std::regex else_if_eq_rev(
        R"(^\s*\}\s*else\s+if\s*\(\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*==\s*(\w+)\s*\)\s*\{)"
    );
    // Regex for: } else {
    static const std::regex else_open(R"(^\s*\}\s*else\s*\{)");
    // Single closing brace
    static const std::regex close_brace(R"(^\s*\}\s*$)");

    // Helper: count net open braces on a line
    auto net_braces = [](const std::string& ln) -> int {
        int d = 0;
        bool in_str = false, in_char = false;
        for (size_t i = 0; i < ln.size(); ++i) {
            char c = ln[i];
            if (c == '"' && !in_char && (i == 0 || ln[i-1] != '\\')) in_str = !in_str;
            else if (c == '\'' && !in_str && (i == 0 || ln[i-1] != '\\')) in_char = !in_char;
            if (!in_str && !in_char) {
                if (c == '{') d++;
                else if (c == '}') d--;
            }
        }
        return d;
    };

    bool changed = false;

    auto try_convert = [&](size_t start_idx) -> size_t {
        std::smatch m;
        std::string var_name, base_indent;

        struct CaseInfo {
            std::string value;
            std::vector<std::string> body;
        };
        std::vector<CaseInfo> cases;
        std::vector<std::string> default_body;
        bool has_default = false;

        // Match first if (var == val) {
        if (std::regex_search(lines[start_idx], m, if_eq_open)) {
            base_indent = m[1].str();
            var_name = m[2].str();
            cases.push_back({m[3].str(), {}});
        } else if (std::regex_search(lines[start_idx], m, if_eq_open_rev)) {
            base_indent = m[1].str();
            var_name = m[3].str();
            cases.push_back({m[2].str(), {}});
        } else {
            return start_idx + 1;
        }

        // Collect body for first case
        int depth = net_braces(lines[start_idx]);
        size_t cur = start_idx + 1;
        while (cur < lines.size() && depth > 0) {
            int d = net_braces(lines[cur]);
            if (depth + d > 0) {
                // Still inside the case body
                cases.back().body.push_back(lines[cur]);
            }
            depth += d;
            cur++;
        }
        // cur now points to line AFTER the closing } (or the closing } line itself)
        // Back up: the closing } line is at cur-1 (if depth went to 0 on that line)
        // Actually, let's reconsider. When depth goes to 0, cur was incremented past.
        // So the closing } is at cur-1. But that line could be "} else if (...) {"
        // which means depth went to 0 and then back to 1.
        // Let me redo: the closing } for the first case is the line where depth
        // first becomes 0. But if it's "} else if (...) {", depth goes to -1+1=0
        // then +1 = 1. So we need to check the same line.
        
        // Let me redo the body collection more carefully:
        // After the opening line, depth = net_braces(opening_line), typically 1.
        // We scan forward until depth drops to 0. The line where it drops to 0
        // is the closing brace line. If it contains "} else if", it also opens
        // the next case.
        
        // Reset and redo:
        cases.back().body.clear();
        depth = net_braces(lines[start_idx]); // typically 1
        cur = start_idx + 1;
        
        // If single-line (depth == 0), body is embedded in the opening line
        if (depth == 0) {
            // Single line: if (var == 0) { return 10; }
            // Extract body from between first { and last }
            std::string full = lines[start_idx];
            size_t open = full.find('{');
            size_t close = full.rfind('}');
            if (open != std::string::npos && close != std::string::npos && close > open + 1) {
                std::string body = full.substr(open + 1, close - open - 1);
                size_t bs = body.find_first_not_of(" \t");
                if (bs != std::string::npos) {
                    body = body.substr(bs);
                    size_t be = body.find_last_not_of(" \t");
                    if (be != std::string::npos) body = body.substr(0, be + 1);
                }
                if (!body.empty()) {
                    cases.back().body.push_back(base_indent + "    " + body);
                }
            }
            // Check next line for continuation
        } else {
            // Multi-line body
            while (cur < lines.size()) {
                int line_d = net_braces(lines[cur]);
                depth += line_d;
                if (depth > 0) {
                    cases.back().body.push_back(lines[cur]);
                    cur++;
                } else {
                    // depth <= 0: this line has the closing }.
                    // Don't add it to body (it's the closing brace line).
                    break;
                }
            }
        }

        // Now look for else-if continuation on the current line (cur) or next line
        size_t chain_end = (depth == 0 && cur == start_idx + 1) ? start_idx : cur;
        if (depth == 0 && cur == start_idx + 1) {
            // Single-line case: next continuation starts at start_idx + 1
            // But only if next line starts with "} else if" — which it won't for single-line.
            // For single-line, the chain would need "if (...) { ... } else if (...) {" all on one line.
            // More likely, it's just followed by another "if" or done.
            // Skip for now — single-line forms are handled by sequential_ifs.
            return start_idx + 1;
        }
        
        // cur points to the line where the closing brace is.
        // Check if it's "} else if (var == val) {" or "} else {"
        while (cur < lines.size()) {
            std::smatch m2;
            if (std::regex_search(lines[cur], m2, else_if_eq)) {
                if (m2[1].str() != var_name) break; // different variable
                cases.push_back({m2[2].str(), {}});
            } else if (std::regex_search(lines[cur], m2, else_if_eq_rev)) {
                if (m2[2].str() != var_name) break;
                cases.push_back({m2[1].str(), {}});
            } else if (std::regex_search(lines[cur], m2, else_open)) {
                has_default = true;
                // Collect default body
                depth = net_braces(lines[cur]);
                cur++;
                while (cur < lines.size()) {
                    int line_d = net_braces(lines[cur]);
                    depth += line_d;
                    if (depth > 0) {
                        default_body.push_back(lines[cur]);
                        cur++;
                    } else {
                        chain_end = cur; // closing } of else block
                        break;
                    }
                }
                break;
            } else {
                break; // No continuation
            }

            // Collect body for this case
            depth = net_braces(lines[cur]);
            cur++;
            while (cur < lines.size()) {
                int line_d = net_braces(lines[cur]);
                depth += line_d;
                if (depth > 0) {
                    cases.back().body.push_back(lines[cur]);
                    cur++;
                } else {
                    chain_end = cur;
                    break;
                }
            }
        }

        // Need at least 3 cases
        if (cases.size() < 3) return start_idx + 1;

        // Build switch statement
        std::ostringstream sw;
        sw << base_indent << "switch (" << var_name << ") {\n";

        for (const auto& ce : cases) {
            sw << base_indent << "case " << ce.value << ":\n";
            for (const auto& bl : ce.body) {
                sw << bl << "\n";
            }
            // Add break unless body ends with return/goto/break
            bool needs_break = true;
            if (!ce.body.empty()) {
                const auto& last = ce.body.back();
                if (last.find("return ") != std::string::npos ||
                    last.find("return;") != std::string::npos ||
                    last.find("goto ") != std::string::npos ||
                    last.find("break;") != std::string::npos) {
                    needs_break = false;
                }
            }
            if (needs_break) sw << base_indent << "    break;\n";
        }

        if (has_default) {
            sw << base_indent << "default:\n";
            for (const auto& dl : default_body) {
                sw << dl << "\n";
            }
        }

        sw << base_indent << "}";

        // Replace lines[start_idx .. chain_end] with the switch
        std::vector<std::string> sw_lines;
        {
            std::istringstream ss(sw.str());
            std::string ln;
            while (std::getline(ss, ln)) sw_lines.push_back(ln);
        }

        size_t erase_end = chain_end + 1;
        if (erase_end > lines.size()) erase_end = lines.size();
        lines.erase(lines.begin() + start_idx, lines.begin() + erase_end);
        lines.insert(lines.begin() + start_idx, sw_lines.begin(), sw_lines.end());

        changed = true;
        fission::utils::log_stream() << "[SwitchReconstructor] Reconstructed switch from if-else-if chain on '"
                  << var_name << "' with " << cases.size() << " cases" << std::endl;

        return start_idx + sw_lines.size();
    };

    size_t idx = 0;
    while (idx < lines.size()) {
        idx = try_convert(idx);
    }

    if (!changed) return c_code;

    std::ostringstream out;
    for (size_t i = 0; i < lines.size(); ++i) {
        out << lines[i];
        if (i + 1 < lines.size()) out << "\n";
    }
    return out.str();
}

// ============================================================================
// Switch Reconstruction from Sequential Equality Checks / BST Patterns
// ============================================================================
//
// Handles two sub-patterns:
//
// 1. Flat sequential equality-return ifs:
//    if (var == 0) { return 10; }
//    if (var == 1) { return 20; }
//    if (var == 2) { return 30; }
//    return default_val;
//
// 2. BST (binary search tree) patterns produced by Ghidra:
//    if (var == 2) { return 30; }
//    if (var < 3) {
//        if (!var) { return 10; }          // var == 0
//        if (var == 1) { return 20; }
//    }
//    return default_val;
//
// In both cases we extract all (var == N) { terminal_stmt } pairs and build
// a switch.

std::string SwitchReconstructor::reconstruct_switch_from_sequential_ifs(const std::string& c_code) {
    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    // Single-line equality-return: if (var == N) { return expr; }
    static const std::regex eq_return(
        R"(^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*\)\s*\{\s*(return\s+[^;]+;)\s*\})"
    );
    static const std::regex eq_return_rev(
        R"(^(\s*)if\s*\(\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*==\s*(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\})"
    );
    // if (!var) { return expr; }  →  var == 0
    static const std::regex not_return(
        R"(^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\})"
    );
    // Range guard: if (var < N) {  or  if (var > N) {
    static const std::regex range_guard_open(
        R"(^(\s*)if\s*\(\s*(\w+)\s*[<>]=?\s*(?:0[xX][0-9A-Fa-f]+|\d+)\s*\)\s*\{)"
    );
    // Closing brace only
    static const std::regex close_brace_only(R"(^\s*\}\s*$)");
    // Return statement (default)
    static const std::regex return_stmt(R"(^\s*(return\s+[^;]+;)\s*$)");

    // Helper: count net braces
    auto net_braces = [](const std::string& ln) -> int {
        int d = 0;
        for (char c : ln) {
            if (c == '{') d++;
            else if (c == '}') d--;
        }
        return d;
    };

    bool changed = false;

    auto try_convert = [&](size_t start_idx) -> size_t {
        // Scan a block of consecutive lines looking for equality-return checks
        // on the same variable, possibly nested inside range guards.
        
        struct CaseInfo {
            std::string value;
            std::string stmt;     // the return/body statement
        };

        std::string var_name, base_indent;
        std::vector<CaseInfo> cases;
        int bst_depth = 0; // nesting depth of range guards
        size_t end_idx = start_idx;
        bool saw_range_guard = false;

        for (size_t i = start_idx; i < lines.size(); ++i) {
            std::smatch m;
            bool matched = false;

            // Try equality-return match
            if (std::regex_match(lines[i], m, eq_return)) {
                std::string vn = m[2].str();
                if (cases.empty()) { var_name = vn; base_indent = m[1].str(); }
                if (vn == var_name) {
                    cases.push_back({m[3].str(), m[4].str()});
                    matched = true;
                }
            } else if (std::regex_match(lines[i], m, eq_return_rev)) {
                std::string vn = m[3].str();
                if (cases.empty()) { var_name = vn; base_indent = m[1].str(); }
                if (vn == var_name) {
                    cases.push_back({m[2].str(), m[4].str()});
                    matched = true;
                }
            } else if (std::regex_match(lines[i], m, not_return)) {
                std::string vn = m[2].str();
                if (cases.empty()) { var_name = vn; base_indent = m[1].str(); }
                if (vn == var_name) {
                    cases.push_back({"0", m[3].str()});
                    matched = true;
                }
            }

            if (matched) {
                end_idx = i;
                continue;
            }

            // Try range guard (BST node): if (var < N) {
            if (!var_name.empty() && std::regex_search(lines[i], m, range_guard_open)) {
                if (m[2].str() == var_name) {
                    bst_depth += net_braces(lines[i]);
                    saw_range_guard = true;
                    end_idx = i;
                    continue;
                }
            }

            // Closing brace of a range guard
            if (bst_depth > 0 && std::regex_match(lines[i], m, close_brace_only)) {
                bst_depth--;
                end_idx = i;
                continue;
            }

            // If we haven't collected any cases yet, skip this line
            if (cases.empty()) return start_idx + 1;

            // We've finished the block of equality checks.
            break;
        }

        // Need at least 3 cases to justify a switch
        if (cases.size() < 3) return start_idx + 1;

        // Check for a default return statement immediately after
        std::string default_stmt;
        bool has_default = false;
        size_t after = end_idx + 1;
        if (after < lines.size()) {
            std::smatch dm;
            if (std::regex_match(lines[after], dm, return_stmt)) {
                default_stmt = dm[1].str();
                has_default = true;
                end_idx = after;
            }
        }

        // Build switch
        std::ostringstream sw;
        sw << base_indent << "switch (" << var_name << ") {\n";
        for (const auto& c : cases) {
            sw << base_indent << "case " << c.value << ":\n";
            sw << base_indent << "    " << c.stmt << "\n";
        }
        if (has_default) {
            sw << base_indent << "default:\n";
            sw << base_indent << "    " << default_stmt << "\n";
        }
        sw << base_indent << "}";

        // Replace
        std::vector<std::string> sw_lines;
        {
            std::istringstream ss(sw.str());
            std::string ln;
            while (std::getline(ss, ln)) sw_lines.push_back(ln);
        }

        size_t erase_end = end_idx + 1;
        lines.erase(lines.begin() + start_idx, lines.begin() + erase_end);
        lines.insert(lines.begin() + start_idx, sw_lines.begin(), sw_lines.end());

        changed = true;
        std::string pattern_type = saw_range_guard ? "BST" : "sequential";
        fission::utils::log_stream() << "[SwitchReconstructor] Reconstructed switch from " << pattern_type
                  << " ifs on '" << var_name << "' with " << cases.size() << " cases" << std::endl;

        return start_idx + sw_lines.size();
    };

    size_t idx = 0;
    while (idx < lines.size()) {
        idx = try_convert(idx);
    }

    if (!changed) return c_code;

    std::ostringstream out;
    for (size_t i = 0; i < lines.size(); ++i) {
        out << lines[i];
        if (i + 1 < lines.size()) out << "\n";
    }
    return out.str();
}

// ============================================================================
// Switch Reconstruction from Bounds-Guarded Equality Chains
// ============================================================================
//
// Handles the common compiler output where a range guard precedes an equality
// dispatch chain that the jump-table reconstructor would otherwise miss:
//
//   if (N < var) goto LAB_default;          // bounds check
//   if (var == 0) goto LAB_0;
//   if (var == 1) goto LAB_1;
//   ...
//   LAB_default:  default_body;
//   LAB_0: case0_body; goto LAB_end;
//   ...
//
// The bounds guard is stripped and LAB_default becomes the switch default:.
// Supported guard forms:
//   if (N < var) goto LABEL;
//   if (var > N) goto LABEL;
//   if ((cast)var > N) goto LABEL;

std::string SwitchReconstructor::reconstruct_switch_from_bounded_chain(const std::string& c_code) {
    // Guard: literal < var  →  captures (indent, literal N, var_name, default_label)
    static const std::regex guard_lt(
        R"(^(\s*)if\s*\(\s*(?:0[xX][0-9A-Fa-f]+|\d+)\s*<\s*(\w+)\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    // Guard: var > literal  →  captures (indent, var_name, literal N, default_label)
    static const std::regex guard_gt(
        R"(^(\s*)if\s*\(\s*(?:\([^)]+\)\s*)?(\w+)\s*>\s*(?:0[xX][0-9A-Fa-f]+|\d+)\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    // Equality check that reconstruct_switch_from_jump_table also uses.
    static const std::regex eq_goto(
        R"(^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    static const std::regex eq_goto_rev(
        R"(^(\s*)if\s*\(\s*(-?(?:0[xX][0-9A-Fa-f]+|\d+))\s*==\s*(\w+)\s*\)\s*goto\s+(\w+)\s*;[ \t]*$)"
    );
    static const std::regex label_line(R"(^(\s*)(\w+)\s*:[ \t]*$)");
    static const std::regex goto_line(R"(^\s*goto\s+(\w+)\s*;[ \t]*$)");

    std::vector<std::string> lines;
    {
        std::istringstream ss(c_code);
        std::string ln;
        while (std::getline(ss, ln)) lines.push_back(ln);
    }

    bool changed = false;

    auto try_convert = [&](size_t start_idx) -> size_t {
        if (start_idx >= lines.size()) return start_idx + 1;

        std::smatch m;
        std::string var_name, base_indent, default_label;

        // Match the bounds guard line
        const std::string& guard_ln = lines[start_idx];
        if (std::regex_match(guard_ln, m, guard_lt)) {
            base_indent   = m[1].str();
            var_name      = m[2].str();
            default_label = m[3].str();
        } else if (std::regex_match(guard_ln, m, guard_gt)) {
            base_indent   = m[1].str();
            var_name      = m[2].str();
            default_label = m[3].str();
        } else {
            return start_idx + 1;
        }

        // Collect the following equality-check chain on the same variable
        struct CaseEntry { std::string value; std::string label; };
        std::vector<CaseEntry> cases;
        size_t i = start_idx + 1;
        for (; i < lines.size(); ++i) {
            const std::string& ln = lines[i];
            if (std::regex_match(ln, m, eq_goto) && m[2].str() == var_name) {
                cases.push_back({m[3].str(), m[4].str()});
            } else if (std::regex_match(ln, m, eq_goto_rev) && m[3].str() == var_name) {
                cases.push_back({m[2].str(), m[4].str()});
            } else {
                break;
            }
        }

        // Need at least 2 equality cases to be worth converting
        if (cases.size() < 2) return start_idx + 1;

        // i now points to the first non-case-check line after the chain.
        // Collect default body: lines between chain end and the first case label.
        std::set<std::string> case_labels;
        for (const auto& c : cases) case_labels.insert(c.label);

        std::vector<std::string> default_lines;
        size_t j = i;
        {
            // If the first post-chain line is a blank goto to the default label,
            // skip it (it's the "fall through to default" path).
            // Otherwise collect until we hit a known case label.
            for (; j < lines.size(); ++j) {
                if (std::regex_match(lines[j], m, label_line)) {
                    if (case_labels.count(m[2].str()) || m[2].str() == default_label)
                        break;
                }
                if (std::regex_match(lines[j], m, goto_line)) {
                    if (!case_labels.count(m[1].str())) continue; // skip exit goto
                }
                default_lines.push_back(lines[j]);
            }
        }

        // Build body map for each case label
        std::map<std::string, std::vector<std::string>> bodies;
        std::string cur_label;
        for (size_t k = j; k < lines.size(); ++k) {
            const std::string& ln = lines[k];
            if (std::regex_match(ln, m, label_line)) {
                std::string lbl = m[2].str();
                if (case_labels.count(lbl)) { cur_label = lbl; continue; }
                if (lbl == default_label)   { cur_label = ""; break; }
            }
            if (!cur_label.empty()) {
                if (std::regex_match(ln, m, goto_line)) {
                    const std::string& tgt = m[1].str();
                    if (!case_labels.count(tgt) && tgt != default_label) {
                        bodies[cur_label].push_back(ln); // real goto inside body
                    }
                    // Skip exit goto — we'll emit break instead.
                    continue;
                }
                bodies[cur_label].push_back(ln);
            }
        }

        // Find end of the construct: the default_label definition line.
        size_t end_idx = j;
        if (!default_label.empty()) {
            for (size_t k = j; k < lines.size(); ++k) {
                if (std::regex_match(lines[k], m, label_line) && m[2].str() == default_label) {
                    end_idx = k + 1; // consume the label line itself
                    break;
                }
            }
        }

        // Build switch statement
        std::ostringstream sw;
        sw << base_indent << "switch (" << var_name << ") {\n";
        for (const auto& ce : cases) {
            sw << base_indent << "case " << ce.value << ":\n";
            auto it = bodies.find(ce.label);
            if (it != bodies.end()) {
                for (const auto& bl : it->second) sw << bl << "\n";
            }
            bool needs_break = true;
            if (it != bodies.end() && !it->second.empty()) {
                const auto& last = it->second.back();
                if (last.find("return ") != std::string::npos ||
                    last.find("goto ")   != std::string::npos ||
                    last.find("break;")  != std::string::npos)
                    needs_break = false;
            }
            if (needs_break) sw << base_indent << "  break;\n";
        }

        bool has_default = false;
        for (const auto& dl : default_lines) {
            std::string t = dl;
            t.erase(0, t.find_first_not_of(" \t"));
            if (!t.empty()) { has_default = true; break; }
        }
        if (has_default) {
            sw << base_indent << "default:\n";
            for (const auto& dl : default_lines) sw << dl << "\n";
        }
        sw << base_indent << "}";

        // Replace lines[start_idx .. end_idx) with the switch text
        std::vector<std::string> sw_lines;
        {
            std::istringstream ss(sw.str());
            std::string ln;
            while (std::getline(ss, ln)) sw_lines.push_back(ln);
        }

        lines.erase(lines.begin() + start_idx, lines.begin() + end_idx);
        lines.insert(lines.begin() + start_idx, sw_lines.begin(), sw_lines.end());

        changed = true;
        fission::utils::log_stream() << "[SwitchReconstructor] Reconstructed bounded-chain switch on '"
                  << var_name << "' with " << cases.size() << " cases (default -> "
                  << default_label << ")" << std::endl;
        return start_idx + sw_lines.size();
    };

    size_t idx = 0;
    while (idx < lines.size()) idx = try_convert(idx);

    if (!changed) return c_code;

    std::ostringstream out;
    for (size_t i = 0; i < lines.size(); ++i) {
        out << lines[i];
        if (i + 1 < lines.size()) out << "\n";
    }
    return out.str();
}

}  // namespace cfg
}  // namespace decompiler
}  // namespace fission
