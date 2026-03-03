#pragma once

#include <string>
#include <vector>

namespace fission {
namespace decompiler {
namespace cfg {

/**
 * @brief Label Analysis - Find and analyze labels and gotos in C code
 * 
 * Provides utilities for extracting labels, goto statements, and determining
 * their relationships (forward/backward jumps, loop headers, etc.)
 */
class LabelAnalyzer {
public:
    struct Label {
        std::string name;
        int line;
        bool is_loop_target;  // backward reference exists
        bool is_used;         // any reference exists
    };
    
    struct GotoInfo {
        std::string target_label;
        int line;
        std::string condition;  // empty if unconditional
        bool is_forward;        // target is after this goto
    };
    
    /**
     * @brief Find all labels in the code (excluding case/default labels)
     * 
     * @param c_code The C source code to analyze
     * @return Vector of label information with line numbers
     */
    static std::vector<Label> find_labels(const std::string& c_code);
    
    /**
     * @brief Find all goto statements (conditional and unconditional)
     * 
     * @param c_code The C source code to analyze
     * @return Vector of goto information with conditions and targets
     */
    static std::vector<GotoInfo> find_gotos(const std::string& c_code);
    
    /**
     * @brief Determine if a label is a loop header (has backward references)
     * 
     * @param label The label name to check
     * @param gotos Vector of all gotos in the code
     * @param labels Vector of all labels in the code
     * @return true if the label has backward references (is a loop target)
     */
    static bool is_loop_header(const std::string& label,
                               const std::vector<GotoInfo>& gotos,
                               const std::vector<Label>& labels);
    
    /**
     * @brief Remove unused labels that are no longer referenced
     * 
     * @param c_code The C source code
     * @return Code with unused labels removed
     */
    static std::string remove_unused_labels(const std::string& c_code);
    
    /**
     * @brief Negate a C boolean condition
     * 
     * Handles simple conditions with comparison operators (==, !=, <, >, <=, >=)
     * and already-negated conditions (!cond).
     * 
     * @param condition The condition to negate
     * @return Negated condition string
     */
    static std::string negate_condition(const std::string& condition);
    
private:
    /**
     * @brief Build sorted vector of newline byte-positions for O(log n) line-number lookup
     */
    static std::vector<size_t> build_newline_index(const std::string& s);
    
    /**
     * @brief Return 1-based line number for byte-position using prebuilt index
     */
    static int pos_to_line(const std::vector<size_t>& nl_idx, size_t pos);
};

} // namespace cfg
} // namespace decompiler
} // namespace fission
