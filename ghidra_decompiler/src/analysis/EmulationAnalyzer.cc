#include "fission/analysis/EmulationAnalyzer.h"
#include <iostream>
#include <sstream>

namespace fission {
namespace analysis {

using namespace ghidra;

// ============================================================================
// EmulationAnalyzer Implementation
// ============================================================================

EmulationAnalyzer::EmulationAnalyzer() {
}

EmulationAnalyzer::~EmulationAnalyzer() {
}

bool EmulationAnalyzer::try_evaluate_condition(PcodeOp* cbranch_op, bool& result) {
    if (!cbranch_op) return false;
    if (cbranch_op->code() != CPUI_CBRANCH) return false;

    // Get the condition varnode (input 1)
    Varnode* cond_vn = cbranch_op->getIn(1);
    if (!cond_vn) return false;

    // If the condition is a constant, we can evaluate it directly
    if (cond_vn->isConstant()) {
        result = (cond_vn->getOffset() != 0);
        return true;
    }

    // Check if it's defined by a simple comparison with a constant
    if (cond_vn->isWritten()) {
        PcodeOp* def_op = cond_vn->getDef();
        if (def_op) {
            OpCode opc = def_op->code();
            // Look for comparison ops
            if (opc == CPUI_INT_EQUAL || opc == CPUI_INT_NOTEQUAL ||
                opc == CPUI_INT_LESS || opc == CPUI_INT_LESSEQUAL ||
                opc == CPUI_INT_SLESS || opc == CPUI_INT_SLESSEQUAL) {
                
                Varnode* in0 = def_op->getIn(0);
                Varnode* in1 = def_op->getIn(1);
                
                // If both are constants, we can fully evaluate
                if (in0 && in1 && in0->isConstant() && in1->isConstant()) {
                    uintb v0 = in0->getOffset();
                    uintb v1 = in1->getOffset();
                    
                    switch (opc) {
                        case CPUI_INT_EQUAL:
                            result = (v0 == v1);
                            return true;
                        case CPUI_INT_NOTEQUAL:
                            result = (v0 != v1);
                            return true;
                        case CPUI_INT_LESS:
                            result = (v0 < v1);
                            return true;
                        case CPUI_INT_LESSEQUAL:
                            result = (v0 <= v1);
                            return true;
                        case CPUI_INT_SLESS:
                            result = ((intb)v0 < (intb)v1);
                            return true;
                        case CPUI_INT_SLESSEQUAL:
                            result = ((intb)v0 <= (intb)v1);
                            return true;
                        default:
                            break;
                    }
                }
            }
        }
    }

    return false;
}

bool EmulationAnalyzer::analyze(Funcdata* fd) {
    if (!fd) return false;
    
    meta_tags.clear();

    // Get the basic block structure
    const BlockGraph& bblocks = fd->getBasicBlocks();
    int num_blocks = bblocks.getSize();
    
    if (num_blocks == 0) return false;

    // Walk all basic blocks looking for CBRANCH ops
    for (int i = 0; i < num_blocks; ++i) {
        FlowBlock* fb = bblocks.getBlock(i);
        if (!fb) continue;
        
        // Only BlockBasic has PcodeOps
        if (fb->getType() != FlowBlock::t_basic) continue;
        
        BlockBasic* bb = (BlockBasic*)fb;
        
        // Get the last op in the block
        PcodeOp* last_op = bb->lastOp();
        if (!last_op) continue;
        
        OpCode opc = last_op->code();
        
        if (opc == CPUI_CBRANCH) {
            // Try to evaluate the condition
            bool condition_result = false;
            bool could_evaluate = try_evaluate_condition(last_op, condition_result);
            
            if (could_evaluate) {
                // Tag this branch with the evaluated result
                std::stringstream ss;
                ss << "[FISSION_META] Condition statically evaluates to: " 
                   << (condition_result ? "TRUE (always taken)" : "FALSE (never taken)");
                meta_tags[last_op->getAddr()] = ss.str();
            }
        }
        else if (opc == CPUI_BRANCHIND || opc == CPUI_CALLIND) {
            // Check if indirect target is constant
            Varnode* target_vn = last_op->getIn(0);
            if (target_vn && target_vn->isConstant()) {
                std::stringstream ss;
                ss << "[FISSION_META] Indirect target is constant: 0x" 
                   << std::hex << target_vn->getOffset();
                meta_tags[last_op->getAddr()] = ss.str();
            }
        }
    }

    // Apply the findings if any
    if (!meta_tags.empty()) {
        apply_tags(fd);
    }

    return !meta_tags.empty();
}

void EmulationAnalyzer::apply_tags(Funcdata* fd) {
    if (meta_tags.empty()) return;
    if (!fd) return;

    // Get the comment database from Architecture
    CommentDatabase* comm_db = fd->getArch()->commentdb;
    if (!comm_db) return;

    Address func_addr = fd->getAddress();

    for (const auto& pair : meta_tags) {
        const Address& addr = pair.first;
        const std::string& msg = pair.second;
        
        // Add as a warning-type comment (stands out in output)
        comm_db->addCommentNoDuplicate(Comment::warning, func_addr, addr, msg);
    }
}

} // namespace analysis
} // namespace fission
