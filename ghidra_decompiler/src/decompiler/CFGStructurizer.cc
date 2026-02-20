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
    
    // Handle comparison operators
    if (cond.find("==") != std::string::npos) {
        return std::regex_replace(cond, std::regex("=="), "!=");
    } else if (cond.find("!=") != std::string::npos) {
        return std::regex_replace(cond, std::regex("!="), "==");
    } else if (cond.find(">=") != std::string::npos) {
        return std::regex_replace(cond, std::regex(">="), "<");
    } else if (cond.find("<=") != std::string::npos) {
        return std::regex_replace(cond, std::regex("<="), ">");
    } else if (cond.find(">") != std::string::npos) {
        return std::regex_replace(cond, std::regex(">"), "<=");
    } else if (cond.find("<") != std::string::npos) {
        return std::regex_replace(cond, std::regex("<"), ">=");
    }
    
    return "!(" + cond + ")";
}

std::vector<CFGStructurizer::Label> CFGStructurizer::find_labels(const std::string& c_code) {
    std::vector<Label> labels;
    // Match labels that appear at start of line or after whitespace
    // But exclude "case" and "default" labels
    std::regex label_pattern(R"((?:^|\n)\s*((?!case\b|default\b)[A-Za-z_]\w*)\s*:(?!\s*:))");
    
    std::string::const_iterator search_start = c_code.cbegin();
    std::smatch match;
    
    while (std::regex_search(search_start, c_code.cend(), match, label_pattern)) {
        size_t pos = match.position() + (search_start - c_code.cbegin());
        int line = std::count(c_code.begin(), c_code.begin() + pos, '\n') + 1;
        
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
    std::regex cond_goto_pattern(R"(if\s*\(([^)]+)\)\s*goto\s+(\w+)\s*;)");
    // Pattern for unconditional goto: goto label;
    std::regex uncond_goto_pattern(R"(\bgoto\s+(\w+)\s*;)");
    
    std::string::const_iterator search_start = c_code.cbegin();
    std::smatch match;
    
    // Find conditional gotos first
    while (std::regex_search(search_start, c_code.cend(), match, cond_goto_pattern)) {
        size_t pos = match.position() + (search_start - c_code.cbegin());
        int line = std::count(c_code.begin(), c_code.begin() + pos, '\n') + 1;
        
        GotoInfo info;
        info.condition = match[1].str();
        info.target_label = match[2].str();
        info.line = line;
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
        
        int line = std::count(c_code.begin(), c_code.begin() + match_pos, '\n') + 1;
        
        GotoInfo info;
        info.condition = "";
        info.target_label = match[1].str();
        info.line = line;
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
    std::regex pattern(
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
    std::regex infinite_loop_pattern(R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)\s*goto\s+\1\s*;)");
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

std::string CFGStructurizer::reconstruct_switch_from_jump_table(const std::string& c_code) {
    // Placeholder for switch reconstruction
    return c_code;
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

