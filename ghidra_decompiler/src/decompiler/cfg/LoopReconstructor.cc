/**
 * LoopReconstructor Implementation
 * 
 * Transforms goto-based loop patterns to structured for/while loops.
 */

#include "fission/decompiler/cfg/LoopReconstructor.h"
#include "fission/decompiler/cfg/LabelAnalyzer.h"
#include <regex>
#include <sstream>
#include <map>

namespace fission {
namespace decompiler {
namespace cfg {

std::string LoopReconstructor::convert_for_loop_patterns(const std::string& c_code) {
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
        else for_cond = LabelAnalyzer::negate_condition(var + " " + op + " " + end_val);
        
        output << indent << "for (" << var << " = " << start_val << "; " 
               << for_cond << "; " << var << "++) {\n"
               << body << indent << "}\n";
        
        last_pos = match.position() + match.length();
    }
    
    output << current.substr(last_pos);
    return output.str();
}

std::string LoopReconstructor::convert_nested_loop_patterns(const std::string& c_code) {
    std::string result = c_code;
    
    // 1. Convert labeled while(true) loops
    // Pattern: LABEL: while(true) { ... if (cond) goto LABEL; ... }
    // This is often used for "continue" in complex loops.
    
    // 2. Identify all backward gotos and transform them into loops if they aren't already
    auto labels = LabelAnalyzer::find_labels(c_code);
    auto gotos = LabelAnalyzer::find_gotos(c_code);
    
    std::map<std::string, int> label_map;
    for (const auto& l : labels) label_map[l.name] = l.line;
    
    std::string transformed = c_code;
    
    // Pattern: LABEL: body; goto LABEL;
    // We already handle this in some way, but let's make it more robust.
    static const std::regex infinite_loop_pattern(R"((\w+)\s*:\s*\n((?:[^\n]*\n)*?)\s*goto\s+\1\s*;)");
    transformed = std::regex_replace(transformed, infinite_loop_pattern, "while (true) {\n$2}\n");
    
    return transformed;
}

std::string LoopReconstructor::eliminate_loop_exits(const std::string& c_code) {
    std::string result = c_code;
    
    // Pattern: while/for/do { ... goto EXIT_LABEL; ... } EXIT_LABEL:
    // Change to break;
    
    // 1. Find all labels
    auto labels = LabelAnalyzer::find_labels(c_code);
    
    for (const auto& label : labels) {
        // Pattern: [while/for/do] { ... goto label; ... } label:
        std::regex break_pattern(R"(\bgoto\s+)" + label.name + R"(\s*;\s*\n?\s*\}\s*\n?\s*)" + label.name + R"(\s*:)");
        result = std::regex_replace(result, break_pattern, "break;\n}\n" + label.name + ":");
    }
    
    return result;
}

std::string LoopReconstructor::normalize_do_while_true(const std::string& c_code) {
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
        
        std::string negated = LabelAnalyzer::negate_condition(condition);
        output << "while (" << negated << ") {\n" << body << "}\n";
        
        search_start = match.suffix().first;
    }
    
    output << std::string(search_start, result.cend());
    return output.str();
}

} // namespace cfg
} // namespace decompiler
} // namespace fission
