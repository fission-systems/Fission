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

std::vector<CFGStructurizer::Label> CFGStructurizer::find_labels(const std::string& c_code) {
    std::vector<Label> labels;
    std::regex label_pattern(R"(^\s*(\w+)\s*:\s*$)", std::regex::multiline);
    
    std::string::const_iterator search_start = c_code.cbegin();
    std::smatch match;
    int line = 1;
    
    while (std::regex_search(search_start, c_code.cend(), match, label_pattern)) {
        // Count lines up to this match
        size_t pos = match.position() + (search_start - c_code.cbegin());
        line = std::count(c_code.begin(), c_code.begin() + pos, '\n') + 1;
        
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
        info.is_forward = true;  // Will be determined later
        gotos.push_back(info);
        
        search_start = match.suffix().first;
    }
    
    // Find unconditional gotos
    search_start = c_code.cbegin();
    while (std::regex_search(search_start, c_code.cend(), match, uncond_goto_pattern)) {
        // Skip if this is part of a conditional goto (already matched)
        std::string prefix = match.prefix().str();
        if (prefix.size() >= 3 && prefix.substr(prefix.size() - 3).find("if") != std::string::npos) {
            search_start = match.suffix().first;
            continue;
        }
        
        size_t pos = match.position() + (search_start - c_code.cbegin());
        int line = std::count(c_code.begin(), c_code.begin() + pos, '\n') + 1;
        
        GotoInfo info;
        info.condition = "";  // Unconditional
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
    // Find the label's line number
    int label_line = -1;
    for (const auto& l : labels) {
        if (l.name == label) {
            label_line = l.line;
            break;
        }
    }
    
    if (label_line == -1) return false;
    
    // Check if any goto targeting this label comes after it (backward jump)
    for (const auto& g : gotos) {
        if (g.target_label == label && g.line > label_line) {
            return true;
        }
    }
    
    return false;
}

// ============================================================================
// Main Transformations
// ============================================================================

std::string CFGStructurizer::eliminate_forward_gotos(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: if (cond) goto LABEL; ... LABEL:
    // This is a forward goto that skips code when condition is true
    // Transform to: if (!cond) { ... }
    
    // Find all conditional gotos
    std::regex pattern(R"(if\s*\(\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;)");
    std::smatch match;
    
    // This is a simplified version - full implementation would need
    // to track label positions and restructure code blocks
    
    // For now, we transform simple patterns where the label is on the next line
    std::regex simple_forward_pattern(
        R"(if\s*\(\s*([^)]+)\s*\)\s*goto\s+(\w+)\s*;\s*\n((?:[^\n]*\n)*?)\s*\2\s*:)"
    );
    
    std::string::const_iterator search_start = result.cbegin();
    std::ostringstream output;
    
    while (std::regex_search(search_start, result.cend(), match, simple_forward_pattern)) {
        // Output everything before the match
        output << match.prefix().str();
        
        std::string condition = match[1].str();
        std::string label = match[2].str();
        std::string body = match[3].str();
        
        // Negate the condition and wrap the body
        // Simple condition negation
        std::string negated_cond;
        if (condition[0] == '!') {
            negated_cond = condition.substr(1);
        } else if (condition.find("==") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex("=="), "!=");
        } else if (condition.find("!=") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex("!="), "==");
        } else if (condition.find(">=") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex(">="), "<");
        } else if (condition.find("<=") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex("<="), ">");
        } else if (condition.find(">") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex(">"), "<=");
        } else if (condition.find("<") != std::string::npos) {
            negated_cond = std::regex_replace(condition, std::regex("<"), ">=");
        } else {
            negated_cond = "!(" + condition + ")";
        }
        
        output << "if (" << negated_cond << ") {\n" << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    // Output the rest
    output << std::string(search_start, result.cend());
    
    return output.str();
}

std::string CFGStructurizer::convert_backward_gotos_to_loops(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern:
    //   LABEL:
    //   body;
    //   if (cond) goto LABEL;
    //
    // Transform to:
    //   do { body; } while (cond);
    
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
        
        // Remove the label and convert to do-while
        output << "do {\n" << body << "} while (" << condition << ");\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    
    return output.str();
}

std::string CFGStructurizer::normalize_do_while_true(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern:
    //   do {
    //     if (cond) break;
    //     body;
    //   } while (true);
    //
    // Transform to:
    //   while (!cond) { body; }
    
    // Simplified pattern - matches the most common form
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
        
        // Negate condition for while loop
        std::string negated_cond;
        if (condition[0] == '!') {
            negated_cond = condition.substr(1);
        } else {
            negated_cond = "!(" + condition + ")";
        }
        
        output << "while (" << negated_cond << ") {\n" << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    
    return output.str();
}

std::string CFGStructurizer::reconstruct_switch_from_jump_table(const std::string& c_code) {
    // This is a placeholder for switch reconstruction
    // Full implementation would analyze computed goto patterns
    // and reconstruct switch statements
    
    return c_code;
}

std::string CFGStructurizer::remove_unused_labels(const std::string& c_code) {
    std::string result = c_code;
    
    // Find all labels
    std::vector<Label> labels = find_labels(c_code);
    
    // Find all gotos
    std::vector<GotoInfo> gotos = find_gotos(c_code);
    
    // Mark used labels
    std::set<std::string> used_labels;
    for (const auto& g : gotos) {
        used_labels.insert(g.target_label);
    }
    
    // Remove unused labels
    for (const auto& label : labels) {
        if (used_labels.find(label.name) == used_labels.end()) {
            // Label is unused, remove it
            std::regex label_pattern(R"(\n\s*)" + label.name + R"(\s*:\s*\n)");
            result = std::regex_replace(result, label_pattern, "\n");
        }
    }
    
    return result;
}

std::string CFGStructurizer::flatten_nested_if_goto(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern:
    //   if (a) {
    //     if (b) {
    //       goto L;
    //     }
    //   }
    //
    // Transform to:
    //   if (a && b) goto L;
    
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
    
    // Apply transformations in order
    // 1. First flatten nested if-goto patterns
    result = flatten_nested_if_goto(result);
    
    // 2. Convert backward gotos to loops (do-while)
    result = convert_backward_gotos_to_loops(result);
    
    // 3. Normalize do-while(true) loops
    result = normalize_do_while_true(result);
    
    // 4. Eliminate forward gotos
    result = eliminate_forward_gotos(result);
    
    // 5. Try switch reconstruction
    result = reconstruct_switch_from_jump_table(result);
    
    // 6. Remove any labels that are no longer used
    result = remove_unused_labels(result);
    
    return result;
}

} // namespace decompiler
} // namespace fission
