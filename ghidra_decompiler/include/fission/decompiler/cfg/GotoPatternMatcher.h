#pragma once

#include <string>

namespace fission {
namespace decompiler {
namespace cfg {

/**
 * @brief Goto Pattern Matcher - Transform goto patterns to structured constructs
 * 
 * Handles various goto patterns:
 * - Forward gotos: if (cond) goto L; ... L: → if (!cond) { ... }
 * - Backward gotos: L: ... if (cond) goto L; → do { ... } while (cond)
 * - Nested if-goto flattening: if (a) { if (b) { goto L; } } → if (a && b) goto L;
 * - Unconditional backward gotos to continue statements
 */
class GotoPatternMatcher {
public:
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
     * 
     * @param c_code The C source code
     * @return Transformed code with forward gotos eliminated
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
     * 
     * Uses CFG analysis to validate natural loops before transformation.
     * 
     * @param c_code The C source code
     * @return Transformed code with backward gotos converted to loops
     */
    static std::string convert_backward_gotos_to_loops(const std::string& c_code);
    
    /**
     * @brief Convert unconditional backward gotos to continue statements
     * 
     * Inside a loop, converts "goto LOOP_LABEL;" to "continue;"
     * 
     * @param c_code The C source code
     * @return Transformed code with continue statements instead of loop gotos
     */
    static std::string convert_unconditional_backward_goto(const std::string& c_code);
    
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
     * 
     * @param c_code The C source code
     * @return Transformed code with flattened nested conditions
     */
    static std::string flatten_nested_if_goto(const std::string& c_code);
};

} // namespace cfg
} // namespace decompiler
} // namespace fission
