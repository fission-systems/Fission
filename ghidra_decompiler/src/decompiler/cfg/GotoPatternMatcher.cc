/**
 * GotoPatternMatcher Implementation
 * 
 * Transforms goto patterns to structured control flow constructs.
 */

#include "fission/decompiler/cfg/GotoPatternMatcher.h"
#include "fission/decompiler/cfg/LabelAnalyzer.h"
#include "fission/analysis/GraphAlgorithms.h"
#include <regex>
#include <sstream>
#include <vector>

namespace fission {
namespace decompiler {
namespace cfg {

using analysis::GraphAnalyzer;

std::string GotoPatternMatcher::eliminate_forward_gotos(const std::string& c_code) {
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
            std::string negated = LabelAnalyzer::negate_condition(condition);
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

std::string GotoPatternMatcher::convert_backward_gotos_to_loops(const std::string& c_code) {
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

std::string GotoPatternMatcher::convert_unconditional_backward_goto(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: while(...) { ... goto LOOP_LABEL; }
    // where LOOP_LABEL is right before the while.
    
    std::vector<LabelAnalyzer::Label> labels = LabelAnalyzer::find_labels(c_code);
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

std::string GotoPatternMatcher::flatten_nested_if_goto(const std::string& c_code) {
    std::string result = c_code;
    
    std::regex pattern(
        R"(if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*if\s*\(\s*([^)]+)\s*\)\s*\{\s*\n\s*goto\s+(\w+)\s*;\s*\n\s*\}\s*\n\s*\})"
    );
    
    result = std::regex_replace(result, pattern, "if ($1 && $2) goto $3;");
    
    return result;
}

} // namespace cfg
} // namespace decompiler
} // namespace fission
