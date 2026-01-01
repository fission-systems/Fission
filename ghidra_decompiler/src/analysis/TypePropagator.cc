#include "fission/analysis/TypePropagator.h"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "type.hh"
#include "architecture.hh"
#include "fspec.hh"
#include <iostream>

namespace fission {
namespace analysis {

using namespace ghidra;

TypePropagator::TypePropagator(Architecture* a) : arch(a) {}
TypePropagator::~TypePropagator() {}

uint64_t TypePropagator::get_varnode_id(Varnode* vn) {
    if (!vn) return 0;
    // Use address space + offset + size as unique ID
    return ((uint64_t)vn->getSpace()->getIndex() << 48) |
           ((uint64_t)vn->getOffset() << 8) |
           (vn->getSize() & 0xFF);
}

void TypePropagator::propagate_from_call(Funcdata* fd, PcodeOp* call_op) {
    if (!call_op || call_op->code() != CPUI_CALL) return;
    
    // Get call target
    Varnode* target = call_op->getIn(0);
    if (!target || !target->isConstant()) return;
    
    uint64_t target_addr = target->getOffset();
    
    // Look up function at target address
    Funcdata* target_func = arch->symboltab->getGlobalScope()->queryFunction(
        Address(arch->getDefaultCodeSpace(), target_addr));
    
    if (!target_func) return;
    
    // Get prototype
    const FuncProto& proto = target_func->getFuncProto();
    int num_params = proto.numParams();
    
    // Map each input parameter to its type
    for (int i = 1; i < call_op->numInput() && i <= num_params; ++i) {
        Varnode* arg = call_op->getIn(i);
        if (!arg) continue;
        
        ProtoParameter* param = proto.getParam(i - 1);
        if (!param) continue;
        
        Datatype* param_type = param->getType();
        if (!param_type || param_type->getMetatype() == TYPE_UNKNOWN) continue;
        
        // Propagate this type backwards
        propagate_backwards(arg, param_type);
    }
}

void TypePropagator::propagate_backwards(Varnode* vn, Datatype* type) {
    if (!vn || !type) return;
    
    uint64_t vid = get_varnode_id(vn);
    if (processed.count(vid)) return;
    processed.insert(vid);
    
    // Store inferred type
    auto it = inferred_types.find(vid);
    if (it == inferred_types.end()) {
        inferred_types[vid] = type;
    } else {
        // Keep more specific type (non-void, has known size)
        if (type->getSize() > it->second->getSize()) {
            inferred_types[vid] = type;
        }
    }
    
    // Follow definition backwards
    PcodeOp* def = vn->getDef();
    if (!def) return;
    
    OpCode opc = def->code();
    
    switch (opc) {
        case CPUI_COPY:
        case CPUI_CAST:
            // Direct copy - propagate to input
            if (def->numInput() > 0) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        case CPUI_LOAD:
            // Load from memory - type applies to loaded value
            // Could propagate to memory location if tracking
            break;
            
        case CPUI_MULTIEQUAL:
            // PHI node - propagate to all inputs
            for (int i = 0; i < def->numInput(); ++i) {
                propagate_backwards(def->getIn(i), type);
            }
            break;
            
        case CPUI_INDIRECT:
            // Indirect - propagate to first input
            if (def->numInput() > 0) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        default:
            // Other ops - stop propagation
            break;
    }
}

void TypePropagator::apply_inferred_types(Funcdata* fd) {
    // Apply types to high-level varnodes
    VarnodeLocSet::const_iterator iter;
    for (iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn) continue;
        
        uint64_t vid = get_varnode_id(vn);
        auto it = inferred_types.find(vid);
        if (it != inferred_types.end() && it->second) {
            // Try to update the high-level variable type
            HighVariable* high = vn->getHigh();
            if (high) {
                // Note: Direct type update on HighVariable is complex
                // For now, we just track the inference
                std::cerr << "[TypePropagator] Inferred type for varnode: " 
                          << it->second->getName() << std::endl;
            }
        }
    }
}

int TypePropagator::propagate(Funcdata* fd) {
    if (!fd) return 0;
    
    clear();
    int count = 0;
    
    // Find all CALL operations
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (op && op->code() == CPUI_CALL) {
            propagate_from_call(fd, op);
            count++;
        }
    }
    
    // Apply inferred types
    if (!inferred_types.empty()) {
        apply_inferred_types(fd);
    }
    
    std::cerr << "[TypePropagator] Analyzed " << count << " calls, inferred " 
              << inferred_types.size() << " types" << std::endl;
    
    return inferred_types.size();
}

Datatype* TypePropagator::get_type(Varnode* vn) {
    if (!vn) return nullptr;
    uint64_t vid = get_varnode_id(vn);
    auto it = inferred_types.find(vid);
    return (it != inferred_types.end()) ? it->second : nullptr;
}

void TypePropagator::clear() {
    inferred_types.clear();
    processed.clear();
}

} // namespace analysis
} // namespace fission
