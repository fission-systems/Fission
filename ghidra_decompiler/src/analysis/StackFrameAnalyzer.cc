#include "fission/analysis/StackFrameAnalyzer.h"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "type.hh"
#include "architecture.hh"
#include <iostream>
#include <algorithm>
#include <sstream>

namespace fission {
namespace analysis {

using namespace ghidra;

StackFrameAnalyzer::StackFrameAnalyzer(Architecture* a) : arch(a) {}
StackFrameAnalyzer::~StackFrameAnalyzer() {}

void StackFrameAnalyzer::collect_stack_accesses(Funcdata* fd) {
    if (!fd) return;
    
    AddrSpace* stack_space = arch->getStackSpace();
    if (!stack_space) return;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        OpCode opc = op->code();
        if (opc != CPUI_LOAD && opc != CPUI_STORE) continue;
        
        // Check if accessing stack
        Varnode* addr_vn = (opc == CPUI_LOAD) ? op->getIn(1) : op->getIn(1);
        if (!addr_vn) continue;
        
        // Look for stack-relative accesses
        PcodeOp* def_op = addr_vn->getDef();
        if (!def_op) continue;
        
        // Common pattern: INT_ADD(stack_pointer, offset)
        if (def_op->code() == CPUI_INT_ADD) {
            Varnode* base = def_op->getIn(0);
            Varnode* offset_vn = def_op->getIn(1);
            
            if (!base || !offset_vn) continue;
            
            // Check if base is stack pointer
            if (base->getSpace() == stack_space || 
                (base->isInput() && base->getSpace()->getName() == "register")) {
                
                if (offset_vn->isConstant()) {
                    int64_t offset = (int64_t)offset_vn->getOffset();
                    int size = (opc == CPUI_LOAD) ? op->getOut()->getSize() : op->getIn(2)->getSize();
                    
                    // Track access
                    auto& entry = stack_accesses[offset];
                    entry.first = std::max(entry.first, size);
                    entry.second++;
                }
            }
        }
    }
}

void StackFrameAnalyzer::cluster_accesses() {
    if (stack_accesses.empty()) return;
    
    // Sort offsets
    std::vector<int64_t> offsets;
    for (const auto& [off, _] : stack_accesses) {
        offsets.push_back(off);
    }
    std::sort(offsets.begin(), offsets.end());
    
    // Cluster by proximity (within 64 bytes = likely same structure)
    const int64_t CLUSTER_THRESHOLD = 64;
    
    StackCluster current;
    current.base_offset = offsets[0];
    current.size = 0;
    
    for (size_t i = 0; i < offsets.size(); ++i) {
        int64_t off = offsets[i];
        int size = stack_accesses[off].first;
        
        if (current.members.empty() || 
            (off - current.base_offset - current.size) <= CLUSTER_THRESHOLD) {
            // Add to current cluster
            StackCluster::Member m;
            m.offset = off - current.base_offset;
            m.size = size;
            m.name = "field_" + std::to_string(m.offset);
            m.type = nullptr;
            current.members.push_back(m);
            current.size = (off - current.base_offset) + size;
        } else {
            // Start new cluster
            if (current.members.size() >= 2) {
                current.inferred_name = "stack_struct_" + std::to_string(clusters.size());
                clusters.push_back(current);
            }
            current = StackCluster();
            current.base_offset = off;
            current.size = size;
            
            StackCluster::Member m;
            m.offset = 0;
            m.size = size;
            m.name = "field_0";
            m.type = nullptr;
            current.members.push_back(m);
        }
    }
    
    // Add last cluster
    if (current.members.size() >= 2) {
        current.inferred_name = "stack_struct_" + std::to_string(clusters.size());
        clusters.push_back(current);
    }
}

TypeStruct* StackFrameAnalyzer::create_struct_for_cluster(TypeFactory* tf, const StackCluster& cluster) {
    if (!tf || cluster.members.empty()) return nullptr;
    
    // Check if already exists
    Datatype* existing = tf->findByName(cluster.inferred_name);
    if (existing) return nullptr;
    
    // Create new structure
    TypeStruct* ts = tf->getTypeStruct(cluster.inferred_name);
    
    // Build fields
    std::vector<TypeField> fields;
    for (const auto& m : cluster.members) {
        Datatype* field_type = m.type;
        if (!field_type) {
            // Default to unsigned of appropriate size
            field_type = tf->getBase(m.size, TYPE_UINT);
        }
        fields.push_back(TypeField(0, m.offset, m.name, field_type));
    }
    
    // Set fields
    if (!fields.empty()) {
        tf->setFields(fields, ts, cluster.size, 0, 0);
    }
    
    return ts;
}

int StackFrameAnalyzer::analyze(Funcdata* fd) {
    clear();
    
    if (!fd) return 0;
    
    collect_stack_accesses(fd);
    cluster_accesses();
    
    std::cerr << "[StackFrameAnalyzer] Found " << stack_accesses.size() 
              << " stack accesses, " << clusters.size() << " structures" << std::endl;
    
    return clusters.size();
}

void StackFrameAnalyzer::apply_structures(TypeFactory* tf) {
    for (const auto& cluster : clusters) {
        TypeStruct* ts = create_struct_for_cluster(tf, cluster);
        if (ts) {
            std::cerr << "[StackFrameAnalyzer] Created " << cluster.inferred_name 
                      << " with " << cluster.members.size() << " fields" << std::endl;
        }
    }
}

void StackFrameAnalyzer::clear() {
    stack_accesses.clear();
    clusters.clear();
}

} // namespace analysis
} // namespace fission
