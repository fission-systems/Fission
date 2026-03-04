#ifndef FISSION_PCODE_EXTRACTOR_H
#define FISSION_PCODE_EXTRACTOR_H

#include <cstdint>
#include "libdecomp.hh"
#include <string>
#include <vector>
#include <map>

namespace fission {
namespace decompiler {

/// Represents a varnode (value in Pcode)
struct VarnodeInfo {
    uint64_t space_id;      // Address space (0=const, 1=unique, 2=register, etc.)
    uint64_t offset;        // Offset within space
    uint32_t size;          // Size in bytes
    bool is_constant;       // Is this a constant value?
    int64_t constant_val;   // Constant value if is_constant
};

/// Represents a Pcode operation with all its details
struct PcodeOpInfo {
    uint32_t seq_num;           // Sequential number in basic block
    std::string opcode;         // Opcode name (COPY, INT_ADD, etc.)
    uint64_t address;           // Instruction address
    
    VarnodeInfo output;         // Output varnode (empty if no output)
    std::vector<VarnodeInfo> inputs; // Input varnodes
};

/// Represents a basic block with its Pcode operations
struct PcodeBasicBlock {
    uint32_t index;             // Block index
    uint64_t start_address;     // First instruction address
    std::vector<PcodeOpInfo> ops; // All Pcode operations
};

/// Main class for extracting Pcode from Ghidra decompilation
class PcodeExtractor {
public:
    /// Extract Pcode operations from a decompiled function
    /// @param fd Ghidra function data
    /// @return JSON string representing the Pcode structure
    static std::string extract_pcode_json(ghidra::Funcdata* fd);
    
    /// Extract Pcode as structured data (for programmatic access)
    /// @param fd Ghidra function data
    /// @return Vector of basic blocks with their Pcode ops
    static std::vector<PcodeBasicBlock> extract_pcode(ghidra::Funcdata* fd);
    
    /// Inject optimized Pcode back into Ghidra (replaces existing Pcode)
    /// @param fd Ghidra function data to modify
    /// @param pcode_json JSON representation of optimized Pcode
    /// @return true if successful, false otherwise
    static bool inject_pcode(ghidra::Funcdata* fd, const std::string& pcode_json);
    
    /// Apply optimized Pcode and regenerate C code
    /// @param fd Ghidra function data
    /// @param pcode_json Optimized Pcode in JSON format
    /// @return Regenerated C code from optimized Pcode
    static std::string apply_optimized_pcode(ghidra::Funcdata* fd, const std::string& pcode_json);
    
private:
    /// Convert Pcode opcode enum to string
    static std::string opcode_to_string(ghidra::OpCode opc);
    
    /// Extract varnode information
    static VarnodeInfo extract_varnode(ghidra::Varnode* vn);
    
    /// Convert Pcode structure to JSON
    static std::string pcode_to_json(const std::vector<PcodeBasicBlock>& blocks);
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_PCODE_EXTRACTOR_H
