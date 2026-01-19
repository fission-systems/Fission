#include "fission/decompiler/PcodeExtractor.h"
#include "fission/utils/logger.h"
#include <sstream>
#include <iomanip>

namespace fission {
namespace decompiler {

std::string PcodeExtractor::opcode_to_string(ghidra::OpCode opc) {
    switch(opc) {
        case ghidra::CPUI_COPY: return "COPY";
        case ghidra::CPUI_LOAD: return "LOAD";
        case ghidra::CPUI_STORE: return "STORE";
        case ghidra::CPUI_BRANCH: return "BRANCH";
        case ghidra::CPUI_CBRANCH: return "CBRANCH";
        case ghidra::CPUI_BRANCHIND: return "BRANCHIND";
        case ghidra::CPUI_CALL: return "CALL";
        case ghidra::CPUI_CALLIND: return "CALLIND";
        case ghidra::CPUI_CALLOTHER: return "CALLOTHER";
        case ghidra::CPUI_RETURN: return "RETURN";
        
        // Arithmetic
        case ghidra::CPUI_INT_EQUAL: return "INT_EQUAL";
        case ghidra::CPUI_INT_NOTEQUAL: return "INT_NOTEQUAL";
        case ghidra::CPUI_INT_SLESS: return "INT_SLESS";
        case ghidra::CPUI_INT_SLESSEQUAL: return "INT_SLESSEQUAL";
        case ghidra::CPUI_INT_LESS: return "INT_LESS";
        case ghidra::CPUI_INT_LESSEQUAL: return "INT_LESSEQUAL";
        case ghidra::CPUI_INT_ZEXT: return "INT_ZEXT";
        case ghidra::CPUI_INT_SEXT: return "INT_SEXT";
        case ghidra::CPUI_INT_ADD: return "INT_ADD";
        case ghidra::CPUI_INT_SUB: return "INT_SUB";
        case ghidra::CPUI_INT_CARRY: return "INT_CARRY";
        case ghidra::CPUI_INT_SCARRY: return "INT_SCARRY";
        case ghidra::CPUI_INT_SBORROW: return "INT_SBORROW";
        case ghidra::CPUI_INT_2COMP: return "INT_2COMP";
        case ghidra::CPUI_INT_NEGATE: return "INT_NEGATE";
        case ghidra::CPUI_INT_XOR: return "INT_XOR";
        case ghidra::CPUI_INT_AND: return "INT_AND";
        case ghidra::CPUI_INT_OR: return "INT_OR";
        case ghidra::CPUI_INT_LEFT: return "INT_LEFT";
        case ghidra::CPUI_INT_RIGHT: return "INT_RIGHT";
        case ghidra::CPUI_INT_SRIGHT: return "INT_SRIGHT";
        case ghidra::CPUI_INT_MULT: return "INT_MULT";
        case ghidra::CPUI_INT_DIV: return "INT_DIV";
        case ghidra::CPUI_INT_SDIV: return "INT_SDIV";
        case ghidra::CPUI_INT_REM: return "INT_REM";
        case ghidra::CPUI_INT_SREM: return "INT_SREM";
        
        // Boolean
        case ghidra::CPUI_BOOL_NEGATE: return "BOOL_NEGATE";
        case ghidra::CPUI_BOOL_XOR: return "BOOL_XOR";
        case ghidra::CPUI_BOOL_AND: return "BOOL_AND";
        case ghidra::CPUI_BOOL_OR: return "BOOL_OR";
        
        // Floating point
        case ghidra::CPUI_FLOAT_EQUAL: return "FLOAT_EQUAL";
        case ghidra::CPUI_FLOAT_NOTEQUAL: return "FLOAT_NOTEQUAL";
        case ghidra::CPUI_FLOAT_LESS: return "FLOAT_LESS";
        case ghidra::CPUI_FLOAT_LESSEQUAL: return "FLOAT_LESSEQUAL";
        case ghidra::CPUI_FLOAT_NAN: return "FLOAT_NAN";
        case ghidra::CPUI_FLOAT_ADD: return "FLOAT_ADD";
        case ghidra::CPUI_FLOAT_DIV: return "FLOAT_DIV";
        case ghidra::CPUI_FLOAT_MULT: return "FLOAT_MULT";
        case ghidra::CPUI_FLOAT_SUB: return "FLOAT_SUB";
        case ghidra::CPUI_FLOAT_NEG: return "FLOAT_NEG";
        case ghidra::CPUI_FLOAT_ABS: return "FLOAT_ABS";
        case ghidra::CPUI_FLOAT_SQRT: return "FLOAT_SQRT";
        
        // Type conversions
        case ghidra::CPUI_FLOAT_INT2FLOAT: return "FLOAT_INT2FLOAT";
        case ghidra::CPUI_FLOAT_FLOAT2FLOAT: return "FLOAT_FLOAT2FLOAT";
        case ghidra::CPUI_FLOAT_TRUNC: return "FLOAT_TRUNC";
        case ghidra::CPUI_FLOAT_CEIL: return "FLOAT_CEIL";
        case ghidra::CPUI_FLOAT_FLOOR: return "FLOAT_FLOOR";
        case ghidra::CPUI_FLOAT_ROUND: return "FLOAT_ROUND";
        
        // Special
        case ghidra::CPUI_MULTIEQUAL: return "MULTIEQUAL";
        case ghidra::CPUI_INDIRECT: return "INDIRECT";
        case ghidra::CPUI_PIECE: return "PIECE";
        case ghidra::CPUI_SUBPIECE: return "SUBPIECE";
        case ghidra::CPUI_CAST: return "CAST";
        case ghidra::CPUI_PTRADD: return "PTRADD";
        case ghidra::CPUI_PTRSUB: return "PTRSUB";
        case ghidra::CPUI_SEGMENTOP: return "SEGMENTOP";
        case ghidra::CPUI_CPOOLREF: return "CPOOLREF";
        case ghidra::CPUI_NEW: return "NEW";
        case ghidra::CPUI_INSERT: return "INSERT";
        case ghidra::CPUI_EXTRACT: return "EXTRACT";
        case ghidra::CPUI_POPCOUNT: return "POPCOUNT";
        
        default: return "UNKNOWN";
    }
}

VarnodeInfo PcodeExtractor::extract_varnode(ghidra::Varnode* vn) {
    VarnodeInfo info;
    
    if (!vn) {
        info.space_id = 0;
        info.offset = 0;
        info.size = 0;
        info.is_constant = false;
        info.constant_val = 0;
        return info;
    }
    
    info.space_id = vn->getSpace()->getIndex();
    info.offset = vn->getOffset();
    info.size = vn->getSize();
    info.is_constant = vn->isConstant();
    
    if (info.is_constant) {
        info.constant_val = vn->getOffset(); // For constants, offset IS the value
    } else {
        info.constant_val = 0;
    }
    
    return info;
}

std::vector<PcodeBasicBlock> PcodeExtractor::extract_pcode(ghidra::Funcdata* fd) {
    std::vector<PcodeBasicBlock> blocks;
    
    if (!fd) return blocks;
    
    // Iterate through basic blocks
    const ghidra::BlockGraph& bblocks = fd->getBasicBlocks();
    for (int i = 0; i < bblocks.getSize(); ++i) {
        ghidra::FlowBlock* fb = bblocks.getBlock(i);
        ghidra::BlockBasic* bb = dynamic_cast<ghidra::BlockBasic*>(fb);
        
        if (!bb) continue;
        
        PcodeBasicBlock block;
        block.index = i;
        block.start_address = bb->getStart().getOffset();
        
        // Iterate through Pcode operations in this block
        std::list<ghidra::PcodeOp*>::const_iterator iter;
        uint32_t seq = 0;
        for (iter = bb->beginOp(); iter != bb->endOp(); ++iter) {
            ghidra::PcodeOp* op = *iter;
            
            PcodeOpInfo op_info;
            op_info.seq_num = seq++;
            op_info.opcode = opcode_to_string(op->code());
            op_info.address = op->getAddr().getOffset();
            
            // Extract output varnode
            ghidra::Varnode* output_vn = op->getOut();
            if (output_vn) {
                op_info.output = extract_varnode(output_vn);
            }
            
            // Extract input varnodes
            for (int j = 0; j < op->numInput(); ++j) {
                ghidra::Varnode* input_vn = op->getIn(j);
                op_info.inputs.push_back(extract_varnode(input_vn));
            }
            
            block.ops.push_back(op_info);
        }
        
        blocks.push_back(block);
    }
    
    return blocks;
}

std::string PcodeExtractor::pcode_to_json(const std::vector<PcodeBasicBlock>& blocks) {
    std::ostringstream json;
    json << "{\"blocks\":[";
    
    for (size_t i = 0; i < blocks.size(); ++i) {
        const PcodeBasicBlock& block = blocks[i];
        if (i > 0) json << ",";
        
        json << "{\"index\":" << block.index 
             << ",\"start_addr\":\"0x" << std::hex << block.start_address << std::dec << "\""
             << ",\"ops\":[";
        
        for (size_t j = 0; j < block.ops.size(); ++j) {
            const PcodeOpInfo& op = block.ops[j];
            if (j > 0) json << ",";
            
            json << "{\"seq\":" << op.seq_num
                 << ",\"opcode\":\"" << op.opcode << "\""
                 << ",\"addr\":\"0x" << std::hex << op.address << std::dec << "\"";
            
            // Output varnode
            if (op.output.size > 0) {
                json << ",\"output\":{\"space\":" << op.output.space_id
                     << ",\"offset\":\"0x" << std::hex << op.output.offset << std::dec << "\""
                     << ",\"size\":" << op.output.size;
                if (op.output.is_constant) {
                    json << ",\"const_val\":" << op.output.constant_val;
                }
                json << "}";
            }
            
            // Input varnodes
            json << ",\"inputs\":[";
            for (size_t k = 0; k < op.inputs.size(); ++k) {
                if (k > 0) json << ",";
                const VarnodeInfo& input = op.inputs[k];
                json << "{\"space\":" << input.space_id
                     << ",\"offset\":\"0x" << std::hex << input.offset << std::dec << "\""
                     << ",\"size\":" << input.size;
                if (input.is_constant) {
                    json << ",\"const_val\":" << input.constant_val;
                }
                json << "}";
            }
            json << "]}";
        }
        
        json << "]}";
    }
    
    json << "]}";
    return json.str();
}

std::string PcodeExtractor::extract_pcode_json(ghidra::Funcdata* fd) {
    std::vector<PcodeBasicBlock> blocks = extract_pcode(fd);
    return pcode_to_json(blocks);
}

bool PcodeExtractor::inject_pcode(ghidra::Funcdata* fd, const std::string& pcode_json) {
    if (!fd) {
        fission::utils::log_stream() << "[PcodeExtractor] Error: null Funcdata" << std::endl;
        return false;
    }
    
    // Parse the JSON to get optimized operations
    // For now, we'll do a simple substitution approach:
    // Find matching operations and replace them with optimized versions
    
    try {
        // This is a simplified approach - full injection would require
        // rebuilding the entire Pcode structure, which is complex
        // Instead, we'll mark operations for transformation
        fission::utils::log_stream() << "[PcodeExtractor] Warning: Full Pcode injection not yet implemented" << std::endl;
        fission::utils::log_stream() << "[PcodeExtractor] Falling back to post-C-generation optimization" << std::endl;
        return false;
    } catch (...) {
        return false;
    }
}

std::string PcodeExtractor::apply_optimized_pcode(ghidra::Funcdata* fd, const std::string& pcode_json) {
    if (!fd) {
        return "{\"status\":\"error\",\"message\":\"null Funcdata\"}";
    }
    
    // Strategy: Since direct Pcode injection is complex, we'll:
    // 1. Extract current Pcode
    // 2. Compare with optimized Pcode
    // 3. Apply transformations at the C generation level
    
    // For now, return empty - this will be enhanced with actual transformation logic
    fission::utils::log_stream() << "[PcodeExtractor] apply_optimized_pcode called (placeholder)" << std::endl;
    return "";
}

} // namespace decompiler
} // namespace fission
