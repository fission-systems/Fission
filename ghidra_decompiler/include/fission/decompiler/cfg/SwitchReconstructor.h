#pragma once

#include <string>

namespace fission {
namespace decompiler {
namespace cfg {

/**
 * @brief Switch Reconstructor - Transform if-chains to switch statements
 * 
 * Detects and reconstructs switch statements from various patterns:
 * 1. Jump table patterns (goto-based dispatch)
 * 2. If-else-if chains with equality checks
 * 3. Sequential if statements with return/terminal statements
 */
class SwitchReconstructor {
public:
    /**
     * @brief Detect and reconstruct switch statements from computed gotos
     * 
     * Pattern:
     *   if (var == 0) goto LAB_case0;
     *   if (var == 1) goto LAB_case1;
     *   <default body>;
     *   LAB_case0: body0; goto LAB_end;
     *   LAB_case1: body1; goto LAB_end;
     *   LAB_end:
     *   
     * Becomes:
     *   switch(var) {
     *   case 0: body0; break;
     *   case 1: body1; break;
     *   default: <default body>;
     *   }
     * 
     * @param c_code The C source code
     * @return Transformed code with switch statement
     */
    static std::string reconstruct_switch_from_jump_table(const std::string& c_code);
    
    /**
     * @brief Reconstruct switch from if-else-if chains
     * 
     * Pattern:
     *   if (var == A) {
     *     body_A;
     *   } else if (var == B) {
     *     body_B;
     *   } else {
     *     default;
     *   }
     *   
     * Becomes:
     *   switch (var) {
     *   case A: body_A; break;
     *   case B: body_B; break;
     *   default: default;
     *   }
     * 
     * @param c_code The C source code
     * @return Transformed code with switch statement
     */
    static std::string reconstruct_switch_from_if_else_chain(const std::string& c_code);
    
    /**
     * @brief Reconstruct switch from sequential equality-check ifs
     * 
     * Handles both flat sequential patterns and BST (binary search tree) patterns
     * produced by optimizing compilers / Ghidra's structure recovery:
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
     *   default: return Z;
     *   }
     * 
     * @param c_code The C source code
     * @return Transformed code with switch statement
     */
    static std::string reconstruct_switch_from_sequential_ifs(const std::string& c_code);

    /**
     * @brief Reconstruct switch from a bounds-guarded equality chain
     *
     * Handles the common compiler output where an unsigned range guard appears
     * before an equality dispatch chain, e.g.:
     *
     *   if (N < var) goto LAB_default;
     *   if (var == 0) goto LAB_0;
     *   if (var == 1) goto LAB_1;
     *   ...
     *
     * Strips the guard and constructs the switch with LAB_default as default:.
     *
     * @param c_code The C source code
     * @return Transformed code with switch statement
     */
    static std::string reconstruct_switch_from_bounded_chain(const std::string& c_code);
};

} // namespace cfg
} // namespace decompiler
} // namespace fission
