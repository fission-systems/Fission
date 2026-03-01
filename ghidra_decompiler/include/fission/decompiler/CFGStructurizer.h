#pragma once

#include <string>
#include <vector>
#include <map>
#include <set>

namespace fission {
namespace decompiler {

/**
 * @brief CFG Structurizer - Converts unstructured control flow to structured form
 * 
 * Inspired by LLVM's StructurizeCFG pass, this transforms goto-laden code into
 * proper if/else/while/for constructs.
 * 
 * Key transformations:
 * 1. Forward goto elimination: goto L; ... L: -> if/else restructuring
 * 2. Backward goto to loop: goto L; ... L: (earlier) -> while/do-while
 * 3. Multi-exit loop normalization: Multiple breaks -> single exit
 * 4. Nested condition flattening: if(a) { if(b) { goto L; } } -> if(a && b) { ... }
 * 
 * The algorithm works on the C source text level using regex and pattern matching,
 * avoiding the need for a full AST.
 */
class CFGStructurizer {
public:
    /**
     * @brief Main entry point - structurize the given C code
     * 
     * @param c_code The decompiled C code with potential gotos
     * @return Structurized C code with gotos replaced by structured constructs
     */
    static std::string structurize(const std::string& c_code);
    
    /**
     * @brief Convert forward gotos to if/else structures
     * 
     * Pattern: 
     *   if (cond) goto label;
     *   stmt1; stmt2; ...
     *   label:
     *   
     * Becomes:
     *   if (!cond) { stmt1; stmt2; ... }
     */
    static std::string eliminate_forward_gotos(const std::string& c_code);
    
    /**
     * @brief Convert backward gotos to while loops
     * 
     * Pattern:
     *   label:
     *   stmt1; stmt2; ...
     *   if (cond) goto label;
     *   
     * Becomes:
     *   do { stmt1; stmt2; ... } while (cond);
     */
    static std::string convert_backward_gotos_to_loops(const std::string& c_code);
    
    /**
     * @brief Convert do-while(true) with break to while(cond)
     * 
     * Pattern:
     *   do {
     *     if (cond) break;
     *     body;
     *   } while(true);
     *   
     * Becomes:
     *   while (!cond) { body; }
     */
    static std::string normalize_do_while_true(const std::string& c_code);
    
    /**
     * @brief Detect and reconstruct switch statements from computed gotos
     * 
     * Pattern:
     *   goto *(&table + idx * 8);
     *   
     * Becomes:
     *   switch(idx) { ... }
     */
    static std::string reconstruct_switch_from_jump_table(const std::string& c_code);
    
    /**
     * @brief Reconstruct switch from if-else-if chains
     * 
     * Pattern:
     *   if (var == A) { body_A; }
     *   else if (var == B) { body_B; }
     *   else { default; }
     *   
     * Becomes:
     *   switch (var) {
     *   case A: body_A; break;
     *   case B: body_B; break;
     *   default: default; }
     */
    static std::string reconstruct_switch_from_if_else_chain(const std::string& c_code);
    
    /**
     * @brief Reconstruct switch from sequential equality-check ifs
     * 
     * Handles both flat sequential patterns and BST (binary search tree) patterns
     * produced by optimising compilers / Ghidra's structure recovery:
     * 
     * Flat:
     *   if (var == A) { return X; }
     *   if (var == B) { return Y; }
     *   return Z;  // default
     * 
     * BST:
     *   if (var == A) { return X; }
     *   if (var < M) { if (var == B) { return Y; } }
     *   return Z;  // default
     *   
     * Becomes:
     *   switch (var) {
     *   case A: return X;
     *   case B: return Y;
     *   default: return Z; }
     */
    static std::string reconstruct_switch_from_sequential_ifs(const std::string& c_code);
    
    /**
     * @brief Remove unnecessary labels that are no longer referenced
     */
    static std::string remove_unused_labels(const std::string& c_code);
    
    /**
     * @brief Simplify nested if-goto patterns
     * 
     * Pattern:
     *   if (a) {
     *     if (b) {
     *       goto L;
     *     }
     *   }
     *   
     * Becomes:
     *   if (a && b) goto L;
     */
    static std::string flatten_nested_if_goto(const std::string& c_code);
    
    /**
     * @brief Convert for-loop patterns from goto structure
     * 
     * Pattern:
     *   i = 0;
     *   LABEL:
     *   if (i >= n) goto EXIT;
     *   body;
     *   i++;
     *   goto LABEL;
     *   EXIT:
     *   
     * Becomes:
     *   for (i = 0; i < n; i++) { body; }
     */
    static std::string convert_for_loop_patterns(const std::string& c_code);
    
    /**
     * @brief Convert nested loop patterns with multiple labels
     * 
     * Handles complex patterns with inner/outer loop labels
     * and converts unconditional backward gotos to loops.
     */
    static std::string convert_nested_loop_patterns(const std::string& c_code);
    
    /**
     * @brief Convert unconditional backward gotos to continue statements
     * 
     * Inside a loop, converts "goto LOOP_LABEL;" to "continue;"
     */
    static std::string convert_unconditional_backward_goto(const std::string& c_code);
    
    /**
     * @brief Convert gotos that exit a loop to break statements
     */
    static std::string eliminate_loop_exits(const std::string& c_code);
    
private:
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
    
    // Helper to parse labels and gotos from code
    static std::vector<Label> find_labels(const std::string& c_code);
    static std::vector<GotoInfo> find_gotos(const std::string& c_code);
    
    // Helper to determine if a label is a loop header (has backward refs)
    static bool is_loop_header(const std::string& label, 
                               const std::vector<GotoInfo>& gotos,
                               const std::vector<Label>& labels);
};

} // namespace decompiler
} // namespace fission
