#include "fission/analysis/EmulationAnalyzer.h"
#include "architecture.hh"
#include <iostream>
#include <sstream>
#include <cstdio>

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

// ============================================================================
// GAP-5: Symbolic constant propagation
// ============================================================================
// Walk backwards through the definition chain of a Varnode, applying simple
// arithmetic/bitwise evaluation, to determine if it reduces to a constant.
// Supports: COPY, ZEXT, SEXT, INT_ADD, INT_SUB, INT_AND, INT_OR, INT_XOR.
// Maximum recursion depth: 4 hops (prevents infinite loops on phi-nodes etc.)
// ============================================================================
bool EmulationAnalyzer::try_propagate_constant(Varnode* vn, uintb& out_val, int depth) {
    if (!vn) return false;
    if (depth > 4) return false;   // cap recursion

    // Already a constant — base case
    if (vn->isConstant()) {
        out_val = vn->getOffset();
        return true;
    }

    if (!vn->isWritten()) return false;

    PcodeOp* def = vn->getDef();
    if (!def) return false;

    OpCode opc = def->code();

    // Handle single-input transparent operations
    switch (opc) {
        case CPUI_COPY:
        case CPUI_INT_ZEXT:
        case CPUI_INT_SEXT:
        case CPUI_CAST: {
            Varnode* in0 = def->getIn(0);
            uintb v;
            if (try_propagate_constant(in0, v, depth + 1)) {
                if (opc == CPUI_INT_SEXT) {
                    // sign-extend to output size
                    int in_bits  = in0->getSize() * 8;
                    int out_bits = vn->getSize() * 8;
                    if (in_bits < out_bits) {
                        uintb sign_bit = (uintb)1 << (in_bits - 1);
                        if (v & sign_bit)
                            v |= (~((uintb)0)) << in_bits;
                    }
                }
                out_val = v;
                return true;
            }
            break;
        }
        // Two-input arithmetic / bitwise
        case CPUI_INT_ADD:
        case CPUI_INT_SUB:
        case CPUI_INT_AND:
        case CPUI_INT_OR:
        case CPUI_INT_XOR:
        case CPUI_INT_LEFT:
        case CPUI_INT_RIGHT: {
            if (def->numInput() < 2) break;
            uintb v0, v1;
            if (!try_propagate_constant(def->getIn(0), v0, depth + 1)) break;
            if (!try_propagate_constant(def->getIn(1), v1, depth + 1)) break;
            switch (opc) {
                case CPUI_INT_ADD:   out_val = v0 + v1;    return true;
                case CPUI_INT_SUB:   out_val = v0 - v1;    return true;
                case CPUI_INT_AND:   out_val = v0 & v1;    return true;
                case CPUI_INT_OR:    out_val = v0 | v1;    return true;
                case CPUI_INT_XOR:   out_val = v0 ^ v1;    return true;
                case CPUI_INT_LEFT:  out_val = v0 << v1;   return true;
                case CPUI_INT_RIGHT: out_val = v0 >> v1;   return true;
                default: break;
            }
            break;
        }
        default:
            break;
    }

    return false;
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
    registered_callind_targets_.clear();

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
            // Check if indirect target can be resolved to a constant (directly
            // or via symbolic constant propagation — GAP-5)
            Varnode* target_vn = last_op->getIn(0);
            if (!target_vn) continue;

            uintb resolved_target = 0;
            bool target_known = false;

            if (target_vn->isConstant()) {
                resolved_target = target_vn->getOffset();
                target_known    = true;
            } else {
                // GAP-5: try up to 4-hop backward walk
                target_known = try_propagate_constant(target_vn, resolved_target, 0);
            }

            if (target_known) {
                std::stringstream ss;
                ss << "[FISSION_META] "
                   << (opc == CPUI_CALLIND ? "Indirect call" : "Indirect branch")
                   << " target resolves to constant: 0x"
                   << std::hex << resolved_target;
                meta_tags[last_op->getAddr()] = ss.str();

                // GAP-5: register the resolved callee as a function stub so
                // the decompiler knows about the target address for future passes.
                if (opc == CPUI_CALLIND
                    && resolved_target != 0
                    && registered_callind_targets_.find(resolved_target) ==
                       registered_callind_targets_.end())
                {
                    Architecture* arch   = fd->getArch();
                    AddrSpace*    cs     = arch ? arch->getDefaultCodeSpace() : nullptr;
                    Scope* global_scope  = (arch && arch->symboltab)
                                              ? arch->symboltab->getGlobalScope()
                                              : nullptr;
                    if (cs && global_scope) {
                        Address target_addr(cs, resolved_target);
                        if (!global_scope->findAddr(target_addr, fd->getAddress())) {
                            char stub_name[32];
                            std::snprintf(stub_name, sizeof(stub_name),
                                          "indirect_%08llx",
                                          (unsigned long long)resolved_target);
                            try {
                                global_scope->addFunction(target_addr, stub_name);
                            } catch (...) {}
                        }
                        registered_callind_targets_.insert(resolved_target);
                    }
                }
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
