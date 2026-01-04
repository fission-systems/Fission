#pragma once

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
     * @brief Apply all post-processing steps
     * 
     * @param c_code The raw decompiled C code
     * @return Processed C code
     */
    static std::string process(const std::string& c_code);
};

} // namespace decompiler
} // namespace fission
