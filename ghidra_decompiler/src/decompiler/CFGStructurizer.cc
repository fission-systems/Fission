/**
 * CFG Structurizer Implementation
 * 
 * Converts unstructured control flow (gotos) to structured constructs.
 * Inspired by LLVM's StructurizeCFG pass but operates on C source code.
 * 
 * This is a best-effort transformation that handles common patterns
 * produced by decompilers.
 */

#include "fission/decompiler/CFGStructurizer.h"
#include "fission/analysis/GraphAlgorithms.h"
#include <regex>
#include <sstream>
#include <algorithm>
#include <iostream>
#include <map>
#include <set>
#include "fission/utils/logger.h"

namespace fission {
namespace decompiler {

using analysis::GraphAnalyzer;
using analysis::Block;
using analysis::Loop;

// ============================================================================
// Helper Functions
// ============================================================================

static std::string negate_condition(const std::string& condition) {
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

// Build sorted vector of newline byte-positions for O(log n) line-number lookup.
static std::vector<size_t> build_newline_index(const std::string& s) {
    std::vector<size_t> idx;
    for (size_t i = 0; i < s.size(); ++i)
        if (s[i] == '\n') idx.push_back(i);
    return idx;
}

// Return 1-based line number for byte-position `pos` using a prebuilt index.
static int pos_to_line(const std::vector<size_t>& nl_idx, size_t pos) {
    // upper_bound gives the number of newlines strictly before `pos`
    return static_cast<int>(
        std::upper_bound(nl_idx.begin(), nl_idx.end(), pos) - nl_idx.begin()) + 1;
}

std::vector<CFGStructurizer::Label> CFGStructurizer::find_labels(const std::string& c_code) {
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

std::vector<CFGStructurizer::GotoInfo> CFGStructurizer::find_gotos(const std::string& c_code) {
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

bool CFGStructurizer::is_loop_header(const std::string& label,
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

// ============================================================================
// Multi-Label Loop Pattern Support
// ============================================================================

/**
 * Convert for-loop patterns:
 *   i = start;
 *   LABEL:
 *   if (i >= end) goto EXIT;
 *   body;
 *   i++;
 *   goto LABEL;
 *   EXIT:
 * 
 * To:
 *   for (i = start; i < end; i++) { body; }
 */
/**
 * Convert for-loop patterns:
 *   i = start;
 *   LABEL:
 *   if (i >= end) goto EXIT;
 *   body;
 *   i++;
 *   goto LABEL;
 *   EXIT:
 * 
 * To:
 *   for (i = start; i < end; i++) { body; }
 */
std::string CFGStructurizer::convert_for_loop_patterns(const std::string& c_code) {
    std::string result = c_code;
    
    // Improved Pattern: handles variable start/end, different operators, and whitespaces
    static const std::regex pattern(
        R"((\w+)\s*=\s*([^;]+)\s*;\s*\n?)"           // i = 0 or i = start_var;
        R"(\s*(\w+)\s*:\s*\n?)"                       // LABEL:
        R"(\s*if\s*\(\s*(\1)\s*(>=|>|<=|<|!=|==)\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n?)"  // if (i >= n) goto EXIT;
        R"(((?:[^\n]*\n)*?))"                          // body
        R"((\s*)(\1)\s*(?:=\s*\1\s*\+\s*1|\+\+)\s*;\s*\n?)"  // i++ or i = i + 1;
        R"(\s*goto\s+\3\s*;\s*\n?)"                    // goto LABEL;
        R"(\s*\6\s*:)"                                  // EXIT:
    );
    
    std::smatch match;
    std::string current = result;
    std::ostringstream output;
    size_t last_pos = 0;
    
    auto it = std::sregex_iterator(current.begin(), current.end(), pattern);
    auto end = std::sregex_iterator();
    
    if (it == end) return result;
    
    for (; it != end; ++it) {
        match = *it;
        output << current.substr(last_pos, match.position() - last_pos);
        
        std::string var = match[1].str();
        std::string start_val = match[2].str();
        std::string loop_label = match[3].str();
        // match[4] is var again
        std::string op = match[5].str();
        std::string end_val = match[6].str();
        std::string exit_label = match[7].str();
        std::string body = match[8].str();
        std::string indent = match[9].str();
        
        // Convert condition to for-loop stayed-in condition
        std::string for_cond;
        if (op == ">=") for_cond = var + " < " + end_val;
        else if (op == ">") for_cond = var + " <= " + end_val;
        else if (op == "<=") for_cond = var + " > " + end_val;
        else if (op == "<") for_cond = var + " >= " + end_val;
        else if (op == "!=") for_cond = var + " == " + end_val;
        else if (op == "==") for_cond = var + " != " + end_val;
        else for_cond = negate_condition(var + " " + op + " " + end_val);
        
        output << indent << "for (" << var << " = " << start_val << "; " 
               << for_cond << "; " << var << "++) {\n"
               << body << indent << "}\n";
        
        last_pos = match.position() + match.length();
    }
    
    output << current.substr(last_pos);
    return output.str();
}

/**
 * Convert nested loop patterns with continue label:
 *   OUTER:
 *   ...
 *   INNER:
 *   ...
 *   if (cond) goto NEXT_OUTER;  // continue outer loop
 *   ...
 *   goto INNER;
 *   NEXT_OUTER:
 *   ...
 *   goto OUTER;
 */
std::string CFGStructurizer::convert_nested_loop_patterns(const std::string& c_code) {
    std::string result = c_code;
    
    // 1. Convert labeled while(true) loops
    // Pattern: LABEL: while(true) { ... if (cond) goto LABEL; ... }
    // This is often used for "continue" in complex loops.
    
    // 2. Identify all backward gotos and transform them into loops if they aren't already
    auto labels = find_labels(c_code);
    auto gotos = find_gotos(c_code);
    
    std::map<std::string, int> label_map;
    for (const auto& l : labels) label_map[l.name] = l.line;
    
    std::string transformed = c_code;
    
    // Pattern: LABEL: body; goto LABEL;
    // We already handle this in some way, but let's make it more robust.
    static const std::regex infinite_loop_pattern(R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)\s*goto\s+\1\s*;)");
    transformed = std::regex_replace(transformed, infinite_loop_pattern, "while (true) {\n$2}\n");
    
    return transformed;
}

/**
 * Convert unconditional backward goto at end of block to continue/break
 */
/**
 * Convert unconditional backward goto at end of block to continue/break
 */
std::string CFGStructurizer::convert_unconditional_backward_goto(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: while(...) { ... goto LOOP_LABEL; }
    // where LOOP_LABEL is right before the while.
    
    std::vector<Label> labels = find_labels(c_code);
    for (const auto& label : labels) {
        // Find if this label is followed by a loop
        std::regex loop_start_pattern(R"()" + label.name + R"(\s*:\s*\n?\s*(?:while|for|do))");
        if (std::regex_search(c_code, loop_start_pattern)) {
            // This is a loop header. Any goto to it inside the loop is a continue.
            std::regex continue_pattern(R"(\bgoto\s+)" + label.name + R"(\s*;\s*\n?\s*\})");
            result = std::regex_replace(result, continue_pattern, "continue;\n}");
        }
    }
    
    return result;
}

std::string CFGStructurizer::eliminate_loop_exits(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: while/for/do { ... goto EXIT_LABEL; ... } EXIT_LABEL:
    // Change to break;
    
    // 1. Find all labels
    auto labels = find_labels(c_code);
    
    for (const auto& label : labels) {
        // Pattern: [while/for/do] { ... goto label; ... } label:
        std::regex break_pattern(R"(\bgoto\s+)" + label.name + R"(\s*;\s*\n?\s*\}\s*\n?\s*)" + label.name + R"(\s*:)");
        result = std::regex_replace(result, break_pattern, "break;\n}\n" + label.name + ":");
    }
    
    return result;
}

// ============================================================================
// Main Transformations
// ============================================================================

std::string CFGStructurizer::eliminate_forward_gotos(const std::string& c_code) {
    std::string result = c_code;
    
    // Improved pattern for forward goto:
    // matches: if (cond) goto LABEL; [optional closing braces/whitespaces] LABEL:
    // This handles skips over code blocks.
    
    // 1. Handle "if (cond) { ... goto LABEL; } ... LABEL:"
    // This is common for error handling or premature exit
    
    // 2. Handle simple skip: if (cond) goto LABEL; body; LABEL:
    // This is what we currently have but let's make it more flexible with braces.
    std::regex skip_pattern(
        R"(if\s*\(\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n((?:[^\n]*\n)*?)\s*(?:\}\s*\n)*\s*\2\s*:)"
    );
    
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), skip_pattern)) {
        std::smatch match;
        std::regex_search(search_start, result.cend(), match, skip_pattern);
        
        output << match.prefix().str();
        
        std::string condition = match[1].str();
        std::string label = match[2].str();
        std::string body = match[3].str();
        
        // If the body is mostly empty or just whitespace/braces, it might be a double jump
        // Only transform if there's actual code being skipped
        bool has_actual_code = false;
        for (char c : body) {
            if (!isspace(c) && c != '}') {
                has_actual_code = true;
                break;
            }
        }
        
        if (has_actual_code) {
            std::string negated = negate_condition(condition);
            output << "if (" << negated << ") {\n" << body << "}\n";
        } else {
            // Just output original if it's too complex or empty skip
            output << match.str();
        }
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

std::string CFGStructurizer::convert_backward_gotos_to_loops(const std::string& c_code) {
    std::string result = c_code;
    
    // 1. Analyze CFG using robust Graph Algorithms
    auto blocks = GraphAnalyzer::build_cfg_from_text(result);
    auto loops = GraphAnalyzer::detect_loops(blocks);
    
    std::ostringstream output;
    std::vector<std::string> lines;
    std::stringstream ss(result);
    std::string line;
    while (std::getline(ss, line)) {
        lines.push_back(line);
    }
    
    // We need to apply transformations. Since multiple loops might exist,
    // working on the line vector is tricky if we insert/remove lines.
    // However, the GraphAnalyzer gave us line numbers valid for 'result'.
    // We can use the detected loops to confirm valid Natural Loops before transforming.
    
    // Pattern: LABEL: body; if (cond) goto LABEL;
    std::regex pattern(
        R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)if\s*\(\s*([^)]+)\s*\)\s*goto\s+\1\s*;)"
    );
    
    std::smatch match;
    std::string::const_iterator search_start = result.cbegin();
    
    while (std::regex_search(search_start, result.cend(), match, pattern)) {
        std::string prefix = match.prefix().str();
        output << prefix;
        
        std::string label = match[1].str();
        std::string body = match[2].str();
        std::string condition = match[3].str();
        
        // Validation: Verify this really constitutes a natural loop in the CFG
        bool is_valid_loop = false;
        
        // Find which loop corresponds to this label
        // The regex match location can be mapped to a block
        // Approximate check: does 'label' appear as a header in our loops?
        for (const auto& loop : loops) {
            const auto& header_blk = blocks[loop.header_id];
            if (header_blk.label == label) {
                // Yes, this label heads a natural loop
                is_valid_loop = true;
                break;
            }
        }
        
        if (is_valid_loop) {
            output << "do {\n" << body << "} while (" << condition << ");\n";
        } else {
            // Not a natural loop (maybe irreducible or cross-jump), keep as goto
            output << match.str();
        }
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

std::string CFGStructurizer::normalize_do_while_true(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: do { if (cond) break; body; } while (true);
    std::regex pattern(
        R"(do\s*\{\s*\n\s*if\s*\(\s*([^)]+)\s*\)\s*(?:break|return[^;]*)\s*;\s*\n((?:[^\}]|\}(?!\s*while))*)\}\s*while\s*\(\s*(?:true|1)\s*\)\s*;)"
    );
    
    std::smatch match;
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), match, pattern)) {
        output << match.prefix().str();
        
        std::string condition = match[1].str();
        std::string body = match[2].str();
        
        std::string negated = negate_condition(condition);
        output << "while (" << negated << ") {\n" << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

// ============================================================================
// Switch Reconstruction
// ============================================================================
//
// Detects patterns emitted by Ghidra for jump-table switches:
//
//   if (var == 0) goto LAB_case0;
//   if (var == 1) goto LAB_case1;
//   if (var == 2) goto LAB_case2;
//   <default body or goto default_label>;
//   goto LAB_end;                        (optional)
//   LAB_case0:
//     body0;
//     goto LAB_end;
//   LAB_case1:
//     body1;
//     goto LAB_end;
//   LAB_case2:
//     body2;
//   LAB_end:
//
// The reconstructed form is:
//   switch (var) {
//   case 0:
//     body0;
//     break;
//   case 1:
//     body1;
//     break;
//   case 2:
//     body2;
//     break;
//   default:
//     <default body>;
//   }
//
// Limitations (known):
//  - Requires all case equality-checks to appear as a contiguous block of
//    "if (var == N) goto LABEL;" lines with the SAME variable name.
//  - Only integer/unsigned literal values (0x..., [0-9]+, negative -[0-9]+)
//    are supported as case values.
//  - Fallthrough between cases (no break) is not synthesised; each case body
//    ends in an explicit break unless the last statement is return/goto.
//  - Default body must appear between the last equality-check and the first
//    case label, or be absent.

std::string CFGStructurizer::reconstruct_switch_from_jump_table(const std::string& c_code) {
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
        fission::utils::log_stream() << "[CFGStructurizer] Reconstructed switch on '" << var_name
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

std::string CFGStructurizer::reconstruct_switch_from_if_else_chain(const std::string& c_code) {
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
        fission::utils::log_stream() << "[CFGStructurizer] Reconstructed switch from if-else-if chain on '"
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

std::string CFGStructurizer::reconstruct_switch_from_sequential_ifs(const std::string& c_code) {
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
        fission::utils::log_stream() << "[CFGStructurizer] Reconstructed switch from " << pattern_type
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

std::string CFGStructurizer::remove_unused_labels(const std::string& c_code) {
    std::string result = c_code;
    
    std::vector<Label> labels = find_labels(c_code);
    std::vector<GotoInfo> gotos = find_gotos(c_code);
    
    std::set<std::string> used_labels;
    for (const auto& g : gotos) {
        used_labels.insert(g.target_label);
    }
    
    for (const auto& label : labels) {
        if (used_labels.find(label.name) == used_labels.end()) {
            std::regex label_pattern(R"(\n\s*)" + label.name + R"(\s*:\s*\n)");
            result = std::regex_replace(result, label_pattern, "\n");
        }
    }
    
    return result;
}

std::string CFGStructurizer::flatten_nested_if_goto(const std::string& c_code) {
    std::string result = c_code;
    
    std::regex pattern(
        R"(if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*goto\s+(\w+)\s*;\s*\n\s*\}\s*\n\s*\})"
    );
    
    result = std::regex_replace(result, pattern, "if ($1 && $2) goto $3;");
    
    return result;
}

// ============================================================================
// Main Entry Point
// ============================================================================

std::string CFGStructurizer::structurize(const std::string& c_code) {
    std::string result = c_code;
    
    int goto_count_before = 0;
    size_t pos = 0;
    while ((pos = result.find("goto ", pos)) != std::string::npos) {
        goto_count_before++;
        pos += 5;
    }
    
    // Apply transformations in order of specificity (most specific first)
    result = flatten_nested_if_goto(result);
    result = convert_for_loop_patterns(result);
    result = convert_backward_gotos_to_loops(result);
    result = convert_nested_loop_patterns(result);
    result = convert_unconditional_backward_goto(result);
    result = eliminate_loop_exits(result);
    result = normalize_do_while_true(result);
    result = eliminate_forward_gotos(result);
    result = reconstruct_switch_from_jump_table(result);
    result = reconstruct_switch_from_if_else_chain(result);
    result = reconstruct_switch_from_sequential_ifs(result);
    result = remove_unused_labels(result);
    
    int goto_count_after = 0;
    pos = 0;
    while ((pos = result.find("goto ", pos)) != std::string::npos) {
        goto_count_after++;
        pos += 5;
    }
    
    if (goto_count_before > goto_count_after) {
        fission::utils::log_stream() << "[CFGStructurizer] Eliminated " << (goto_count_before - goto_count_after) 
                  << " gotos (" << goto_count_before << " -> " << goto_count_after << ")" << std::endl;
    }
    
    return result;
}

} // namespace decompiler
} // namespace fission

