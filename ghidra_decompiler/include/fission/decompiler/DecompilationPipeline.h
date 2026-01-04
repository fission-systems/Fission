#ifndef FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H
#define FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H

#include <string>
#include "fission/core/DecompilerContext.h"

namespace fission {
namespace decompiler {

/**
 * @brief Core decompilation pipeline
 * 
 * Manages the complete decompilation workflow:
 * - Binary loading and initialization
 * - Multi-phase analysis (RTTI, VTable, FID, patterns)
 * - Decompilation execution
 * - Structure recovery and type propagation
 * - Post-processing and output generation
 */
class DecompilationPipeline {
public:
    /**
     * @brief Process a single decompilation request
     * 
     * Handles both load_bin commands and normal decompilation requests.
     * Executes multi-step pipeline with error recovery.
     * 
     * @param state Decompiler context with cached state
     * @param input JSON request string
     * @return JSON response string (status + code/message)
     */
    static std::string process_request(fission::core::DecompilerContext& state, const std::string& input);

private:
    /**
     * @brief Handle binary loading command
     * 
     * Initializes architecture, runs analysis phases:
     * - Binary format detection (PE/ELF/Mach-O)
     * - RTTI and VTable recovery
     * - Pattern matching and FID database loading
     * - String scanning and symbol injection
     * 
     * @param state Decompiler context to populate
     * @param input JSON request with load_bin command
     * @return JSON response (status + message)
     */
    static std::string handle_load_bin(fission::core::DecompilerContext& state, const std::string& input);
    
    /**
     * @brief Handle normal decompilation request
     * 
     * Decompiles single function at specified address:
     * - Setup architecture and memory
     * - Execute decompilation actions
     * - Apply structure recovery
     * - Perform reverse type propagation
     * - Generate and post-process C code
     * 
     * @param state Decompiler context with initialized binary
     * @param input JSON request with address and bytes
     * @return JSON response (status + C code)
     */
    static std::string handle_decompile(fission::core::DecompilerContext& state, const std::string& input);
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H
