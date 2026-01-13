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
#include <regex>
#include <sstream>
#include <algorithm>
#include <iostream>

namespace fission {
namespace decompiler {

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
    std::regex label_pattern(R"((?:^|\n)\s*((?!case\b|default\b)[A-Za-z_]\w*)\s*:(?!\s*:))", std::regex::multiline);
    
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
std::string CFGStructurizer::convert_for_loop_patterns(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: init; LABEL: if(cond) goto EXIT; body; incr; goto LABEL; EXIT:
    std::regex pattern(
        R"((\w+)\s*=\s*(\d+)\s*;\s*\n)"          // i = 0;
        R"(\s*(\w+)\s*:\s*\n)"                    // LABEL:
        R"(\s*if\s*\(\s*(\w+)\s*(>=|>|<=|<)\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n)"  // if (i >= n) goto EXIT;
        R"(((?:[^\n]*\n)*?))"                     // body
        R"(\s*\4\s*(?:=\s*\4\s*\+\s*1|\+\+)\s*;\s*\n)"  // i++ or i = i + 1;
        R"(\s*goto\s+\3\s*;\s*\n)"                // goto LABEL;
        R"(\s*\7\s*:)"                             // EXIT:
    );
    
    std::smatch match;
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), match, pattern)) {
        output << match.prefix().str();
        
        std::string var = match[1].str();
        std::string start_val = match[2].str();
        std::string loop_label = match[3].str();
        std::string loop_var = match[4].str();
        std::string op = match[5].str();
        std::string end_val = match[6].str();
        std::string exit_label = match[7].str();
        std::string body = match[8].str();
        
        // Convert condition
        std::string for_cond;
        if (op == ">=") for_cond = loop_var + " < " + end_val;
        else if (op == ">") for_cond = loop_var + " <= " + end_val;
        else if (op == "<=") for_cond = loop_var + " > " + end_val;
        else if (op == "<") for_cond = loop_var + " >= " + end_val;
        else for_cond = "!(" + loop_var + " " + op + " " + end_val + ")";
        
        output << "for (" << var << " = " << start_val << "; " 
               << for_cond << "; " << loop_var << "++) {\n"
               << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
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
    
    // First, identify all loop headers (labels with backward gotos)
    auto labels = find_labels(c_code);
    auto gotos = find_gotos(c_code);
    
    for (auto& label : labels) {
        label.is_loop_target = is_loop_header(label.name, gotos, labels);
    }
    
    // Convert simple patterns: LABEL: body; goto LABEL; -> do { body; } while(true);
    // This handles unconditional backward gotos
    std::regex uncond_loop_pattern(
        R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)\s*goto\s+\1\s*;)"
    );
    
    std::smatch match;
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), match, uncond_loop_pattern)) {
        output << match.prefix().str();
        
        std::string label = match[1].str();
        std::string body = match[2].str();
        
        // Check if body contains a break condition
        std::regex break_pattern(R"(if\s*\(\s*([^)]+)\s*\)\s*(?:break|goto\s+\w+)\s*;)");
        std::smatch break_match;
        
        if (std::regex_search(body, break_match, break_pattern)) {
            // Has break condition - transform to while loop
            std::string break_cond = break_match[1].str();
            std::string remaining_body = break_match.prefix().str() + 
                                          break_match.suffix().str();
            output << "while (" << negate_condition(break_cond) << ") {\n"
                   << remaining_body << "}\n";
        } else {
            // No break condition - keep as do-while(true)
            output << "do {\n" << body << "} while (true);\n";
        }
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

/**
 * Convert unconditional backward goto at end of block to continue/break
 */
std::string CFGStructurizer::convert_unconditional_backward_goto(const std::string& c_code) {
    std::string result = c_code;
    
    // Find labels and determine which are loop headers
    auto labels = find_labels(c_code);
    auto gotos = find_gotos(c_code);
    
    std::set<std::string> loop_labels;
    for (const auto& label : labels) {
        if (is_loop_header(label.name, gotos, labels)) {
            loop_labels.insert(label.name);
        }
    }
    
    // Inside a loop, convert "goto LOOP_LABEL;" to "continue;"
    // This is a simplified version - full implementation would need
    // proper scope tracking
    
    for (const auto& loop_label : loop_labels) {
        // Pattern: inside do { } while, "goto LABEL;" -> "continue;"
        std::regex pattern(
            R"((do\s*\{[^}]*?)goto\s+)" + loop_label + R"(\s*;([^}]*\}\s*while))"
        );
        result = std::regex_replace(result, pattern, "$1continue;$2");
    }
    
    return result;
}

// ============================================================================
// Main Transformations
// ============================================================================

std::string CFGStructurizer::eliminate_forward_gotos(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: if (cond) goto LABEL; ... LABEL:
    std::regex simple_forward_pattern(
        R"(if\s*\(\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n((?:[^\n]*\n)*?)\s*\2\s*:)"
    );
    
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), simple_forward_pattern)) {
        std::smatch match;
        std::regex_search(search_start, result.cend(), match, simple_forward_pattern);
        
        output << match.prefix().str();
        
        std::string condition = match[1].str();
        std::string label = match[2].str();
        std::string body = match[3].str();
        
        std::string negated = negate_condition(condition);
        output << "if (" << negated << ") {\n" << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

std::string CFGStructurizer::convert_backward_gotos_to_loops(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: LABEL: body; if (cond) goto LABEL;
    std::regex pattern(
        R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)if\s*\(\s*([^)]+)\s*\)\s*goto\s+\1\s*;)"
    );
    
    std::smatch match;
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), match, pattern)) {
        output << match.prefix().str();
        
        std::string label = match[1].str();
        std::string body = match[2].str();
        std::string condition = match[3].str();
        
        output << "do {\n" << body << "} while (" << condition << ");\n";
        
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
    
    // Apply transformations in order of specificity (most specific first)
    
    // 1. Flatten nested if-goto patterns
    result = flatten_nested_if_goto(result);
    
    // 2. Try to recognize for-loop patterns (most structured)
    result = convert_for_loop_patterns(result);
    
    // 3. Convert backward gotos to do-while loops
    result = convert_backward_gotos_to_loops(result);
    
    // 4. Handle nested loop patterns with multiple labels
    result = convert_nested_loop_patterns(result);
    
    // 5. Convert unconditional backward gotos to continue
    result = convert_unconditional_backward_goto(result);
    
    // 6. Normalize do-while(true) to while loops
    result = normalize_do_while_true(result);
    
    // 7. Eliminate forward gotos
    result = eliminate_forward_gotos(result);
    
    // 8. Try switch reconstruction
    result = reconstruct_switch_from_jump_table(result);
    
    // 9. Remove any labels that are no longer used
    result = remove_unused_labels(result);
    
    return result;
}

} // namespace decompiler
} // namespace fission

