#pragma once

#include <string>

namespace fission {
namespace decompiler {

/**
 * @brief CFG Structurizer - Converts unstructured control flow to structured form
 * 
 * Inspired by LLVM's StructurizeCFG pass, this transforms goto-laden code into
 * proper if/else/while/for constructs.
 * 
 * This is the main orchestrator that delegates to specialized passes:
 * - LabelAnalyzer: Label and goto extraction/analysis
 * - GotoPatternMatcher: Forward/backward goto transformations
 * - LoopReconstructor: For/while loop reconstruction
 * - SwitchReconstructor: Switch statement reconstruction
 * 
 * The algorithm works on the C source text level using regex and pattern matching,
 * avoiding the need for a full AST.
 */
class CFGStructurizer {
public:
    /**
     * @brief Main entry point - structurize the given C code
     * 
     * Applies transformations in order of specificity (most specific first):
     * 1. Flatten nested if-goto patterns
     * 2. Convert for-loop patterns
     * 3. Convert backward gotos to loops
     * 4. Convert nested loop patterns
     * 5. Convert unconditional backward gotos to continue
     * 6. Eliminate loop exits (goto → break)
     * 7. Normalize do-while(true) to while(cond)
     * 8. Eliminate forward gotos
     * 9. Reconstruct switch from jump tables
     * 10. Reconstruct switch from if-else chains
     * 11. Reconstruct switch from sequential ifs
     * 12. Remove unused labels
     * 
     * @param c_code The decompiled C code with potential gotos
     * @return Structurized C code with gotos replaced by structured constructs
     */
    static std::string structurize(const std::string& c_code);
};

} // namespace decompiler
} // namespace fission
