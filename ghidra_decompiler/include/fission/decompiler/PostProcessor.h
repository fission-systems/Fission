#pragma once

#include <cstdint>
#include <string>

namespace fission {
namespace decompiler {

/**
 * @brief Post-processing utilities for decompiled C code
 * 
 * Provides functions to enhance decompiled output by:
 * - Converting integer constants to string literals
 * - Cleaning up redundant casts
 * - Formatting output
 */
class PostProcessor {
public:
    /**
     * @brief Convert integer constants to string literals if they look like ASCII
     * 
     * Converts hex values like 0x6d65744974736554 to (QWORD)"TestItem" if the
     * bytes form readable ASCII strings.
     * 
     * @param c_code The C code string to process
     * @return Modified C code with string literals
     */
    static std::string convert_integer_constants(std::string c_code);
    
    /**
     * @brief Convert while(true) loops to for loops when induction variable is detected
     * 
     * Transforms patterns like:
     *   i = 0;
     *   while(true) { if(i >= n) break; ...; i = i + 1; }
     * Into:
     *   for(i = 0; i < n; i++) { ... }
     * 
     * @param c_code The C code string to process
     * @return Modified C code with for loops
     */
    static std::string convert_while_to_for(std::string c_code);
    
    /**
     * @brief Simplify nested if statements
     * 
     * Transforms: if(a) { if(b) { ... } } -> if(a && b) { ... }
     * 
     * @param c_code The C code string to process
     * @return Modified C code with simplified conditions
     */
    static std::string simplify_nested_if(std::string c_code);
    
    /**
     * @brief Fold sequential local variable assignments into array initializers
     * 
     * Transforms:
     *   local_28 = 1; local_24 = 2; local_20 = 3; local_1c = 4;
     * Into:
     *   int arr[4] = {1, 2, 3, 4};
     * 
     * @param c_code The C code string to process
     * @return Modified C code with array initializers
     */
    static std::string fold_array_init(std::string c_code);
    
    /**
     * @brief Improve variable names based on usage context
     * 
     * Renames local_XX to more meaningful names like:
     * - loop_idx when used as loop counter
     * - str_ptr when used with string functions
     * - result when assigned before return
     * 
     * @param c_code The C code string to process
     * @return Modified C code with improved variable names  
     */
    static std::string improve_variable_names(std::string c_code);
    
    /**
     * @brief Structurize control flow - eliminate gotos and normalize loops
     * 
     * Uses LLVM-inspired CFG structurization algorithms to:
     * - Convert backward gotos to do-while/while loops
     * - Convert forward gotos to if/else structures
     * - Normalize do-while(true) with break to while(cond)
     * - Remove unused labels
     * 
     * @param c_code The C code string to process
     * @return Modified C code with structured control flow
     */
    static std::string structurize_control_flow(std::string c_code);

    /**
     * @brief Convert while-with-init-and-increment patterns to for loops.
     *
     * Detects:
     *   VAR = INIT;
     *   while (VAR OP END) { ...body...; VAR++; }
     * and transforms to:
     *   for (VAR = INIT; VAR OP END; VAR++) { ...body... }
     *
     * @param c_code The C code string to process
     * @return Modified C code with for loops where possible
     */
    static std::string convert_while_to_for_struct(std::string c_code);

    /**
     * @brief Remove redundant / widening casts and replace (void*)0 with NULL
     *
     * Handles:
     *   (ulonglong)(uint)x   → (uint)x    (widening wrapper is display noise)
     *   (longlong)(int)x     → (int)x
     *   (int)(int)x          → (int)x     (same-type double cast)
     *   (void*)0 / (void *)0x0 → NULL
     *
     * @param c_code The C code string to process
     * @return Modified C code with simplified casts
     */
    static std::string eliminate_redundant_casts(std::string c_code);

    /**
     * @brief Remove trivially-dead / self-assignment statements
     *
     * Deletes whole lines that are of the form:
     *   x = x;   /   local_8 = local_8;
     *
     * @param c_code The C code string to process
     * @return Modified C code with self-assignments removed
     */
    static std::string eliminate_dead_stores(std::string c_code);

    /**
     * @brief Rewrite pointer arithmetic to array subscript notation
     *
     * Detects common Ghidra output patterns and converts to readable form:
     *
     *   *(int *)((char *)ptr + 8)                    →  ((int *)ptr)[2]
     *   *(uint32_t *)((longlong)base + (longlong)i*4) →  base[i]
     *   *(uint32_t *)(base + i * 4)                  →  base[i]
     *   *(uint64_t *)((longlong)base + 16)            →  ((uint64_t *)base)[2]
     *
     * Only fires when the byte offset is evenly divisible by sizeof(T),
     * ensuring output is semantically equivalent to input.
     *
     * Should run before eliminate_redundant_casts() to give that pass a
     * chance to clean up any remaining (longlong)/(char *) wrappers.
     *
     * @param c_code The C code string to process
     * @return Modified C code with array subscript notation
     */
    static std::string rewrite_pointer_arithmetic_to_array(std::string c_code);

    /**
     * @brief Apply all post-processing steps
     * 
     * Order of processing:
     * 1. convert_integer_constants - Extract string literals
     * 2. structurize_control_flow - Eliminate gotos
     * 3. convert_while_to_for_struct - while+init+inc → for loops
     * 4. convert_while_to_for - Compound operators
     * 5. simplify_nested_if - Condition simplification
     * 6. fold_array_init - Array detection
     * 7. improve_variable_names - Variable renaming
     * 
     * @param c_code The raw decompiled C code
     * @return Processed C code
     */
    static std::string process(const std::string& c_code);
};

} // namespace decompiler
} // namespace fission
