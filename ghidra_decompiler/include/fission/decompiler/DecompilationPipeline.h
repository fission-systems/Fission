#ifndef FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H
#define FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H

#include <cstdint>
#include <string>
#include <vector>
#include "fission/core/DecompilerContext.h"
#include "fission/loader/BinaryDetector.h"

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

    // ── handle_load_bin sub-phases (decomposed from the monolithic God function) ───

    /**
     * @brief Phase 1 – Detect binary format, arch, and bitness.
     *
     * Uses the optional sleigh_id/compiler_id hints from the JSON request
     * (via parse_sleigh_id()) and falls back to BinaryDetector::detect().
     *
     * @param input  Raw JSON request
     * @param bytes  Binary bytes
     * @return Populated BinaryInfo (format, arch, sleigh_id, compiler_id, is_64bit)
     */
    static fission::loader::BinaryInfo detect_binary_info(
        const std::string& input,
        const std::vector<uint8_t>& bytes);

    /**
     * @brief Phases 3-9.5 – Run pre-decompilation analysis (RTTI, VTable, FID, strings…).
     *
     * Populates state.iat_symbols, state.fid_function_names, state.vtable_virtual_names,
     * state.enum_values and sets up the Ghidra architecture objects.
     *
     * @param state        Decompiler context to populate
     * @param info         Binary info from detect_binary_info()
     * @param bytes        Full binary bytes
     * @param image_base   Load address
     * @param compiler_id  Compiler/OS string (e.g. "windows", "gcc")
     */
    static void run_preanalysis(
        fission::core::DecompilerContext& state,
        const fission::loader::BinaryInfo& info,
        const std::vector<uint8_t>& bytes,
        uint64_t image_base,
        const std::string& compiler_id);

    /**
     * @brief Phases 10-12 – IAT injection + FID/InternalMatcher prologue scan.
     *
     * Finalises symbol tables for both 32- and 64-bit architectures and runs
     * the unified prologue/FID matching pass.
     *
     * @param state        Decompiler context (arch already initialised)
     * @param info         Binary info from detect_binary_info()
     * @param bytes        Full binary bytes
     * @param image_base   Load address
     * @param compiler_id  Compiler/OS string
     * @param input        Original JSON request (for IAT symbol extraction)
     */
    static void run_signature_analysis(
        fission::core::DecompilerContext& state,
        const fission::loader::BinaryInfo& info,
        const std::vector<uint8_t>& bytes,
        uint64_t image_base,
        const std::string& compiler_id,
        const std::string& input);
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_DECOMPILATION_PIPELINE_H
