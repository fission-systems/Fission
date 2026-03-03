#pragma once

#include <string>

namespace fission {
namespace decompiler {
namespace cfg {

/**
 * @brief Loop Reconstructor - Transform gotos to structured loops
 * 
 * Handles various loop patterns:
 * - For-loop reconstruction from goto patterns with induction variables
 * - Nested loop patterns with multiple labels
 * - Loop exit conversion (goto → break)
 * - do-while(true) normalization to while(cond)
 */
class LoopReconstructor {
public:
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
     * 
     * @param c_code The C source code
     * @return Transformed code with for-loops instead of goto-based loops
     */
    static std::string convert_for_loop_patterns(const std::string& c_code);
    
    /**
     * @brief Convert nested loop patterns with multiple labels
     * 
     * Handles complex patterns with inner/outer loop labels and converts
     * unconditional backward gotos to loops.
     * 
     * Pattern:
     *   LABEL: body; goto LABEL;
     *   
     * Becomes:
     *   while (true) { body; }
     * 
     * @param c_code The C source code
     * @return Transformed code with nested loops structured
     */
    static std::string convert_nested_loop_patterns(const std::string& c_code);
    
    /**
     * @brief Convert gotos that exit a loop to break statements
     * 
     * Pattern:
     *   while/for/do { ... goto EXIT_LABEL; ... } EXIT_LABEL:
     *   
     * Becomes:
     *   while/for/do { ... break; ... } EXIT_LABEL:
     * 
     * @param c_code The C source code
     * @return Transformed code with break statements instead of exit gotos
     */
    static std::string eliminate_loop_exits(const std::string& c_code);
    
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
     * 
     * @param c_code The C source code
     * @return Transformed code with simplified while loops
     */
    static std::string normalize_do_while_true(const std::string& c_code);
};

} // namespace cfg
} // namespace decompiler
} // namespace fission
