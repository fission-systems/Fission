#include "fission/analysis/StackFrameAnalyzer.h"
#include "fission/types/TypeResolver.h"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "type.hh"
#include "architecture.hh"
#include "address.hh"
#include <iostream>
#include "fission/utils/logger.h"
#include <algorithm>
#include <sstream>

namespace fission {
namespace analysis {

using namespace ghidra;

StackFrameAnalyzer::StackFrameAnalyzer(Architecture* a) : arch(a) {}
StackFrameAnalyzer::~StackFrameAnalyzer() {}

static bool get_signed_offset(ghidra::Varnode* vn, int64_t& out) {
    if (!vn || !vn->isConstant()) {
        return false;
    }
    int size = vn->getSize();
    if (size <= 0) {
        return false;
    }
    int bits = (size * 8) - 1;
    ghidra::intb raw = static_cast<ghidra::intb>(vn->getOffset());
    out = static_cast<int64_t>(ghidra::sign_extend(raw, bits));
    return true;
}

static bool is_stack_base(ghidra::Varnode* vn, ghidra::AddrSpace* stack_space) {
    if (!vn || !stack_space) return false;
    if (vn->getSpace() == stack_space) return true;
    if (vn->isInput() && vn->getSpace()->getName() == "register") return true;
    return false;
}

static int64_t normalize_stack_offset(ghidra::AddrSpace* stack_space, uint64_t offset) {
    if (!stack_space) {
        return static_cast<int64_t>(offset);
    }
    int bytes = stack_space->getAddrSize();
    if (bytes <= 0) {
        return static_cast<int64_t>(offset);
    }
    int bits = (bytes * 8) - 1;
    ghidra::intb raw = static_cast<ghidra::intb>(offset);
    return static_cast<int64_t>(ghidra::sign_extend(raw, bits));
}

static bool resolve_stack_offset(
    ghidra::Varnode* vn,
    ghidra::AddrSpace* stack_space,
    int64_t& offset,
    int depth = 6
) {
    if (!vn || depth <= 0) {
        return false;
    }
    if (is_stack_base(vn, stack_space)) {
        if (vn->getSpace() == stack_space) {
            offset += static_cast<int64_t>(vn->getOffset());
        }
        return true;
    }
    if (!vn->isWritten()) {
        return false;
    }
    ghidra::PcodeOp* def = vn->getDef();
    if (!def) {
        return false;
    }

    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            return resolve_stack_offset(def->getIn(0), stack_space, offset, depth - 1);
        case ghidra::CPUI_PTRSUB: {
            ghidra::Varnode* base = def->getIn(0);
            int64_t off = 0;
            if (!get_signed_offset(def->getIn(1), off)) {
                return false;
            }
            offset += off;
            return resolve_stack_offset(base, stack_space, offset, depth - 1);
        }
        case ghidra::CPUI_PTRADD: {
            ghidra::Varnode* base = def->getIn(0);
            int64_t idx = 0;
            int64_t elem = 0;
            if (!get_signed_offset(def->getIn(1), idx)) {
                return false;
            }
            if (!get_signed_offset(def->getIn(2), elem)) {
                return false;
            }
            offset += idx * elem;
            return resolve_stack_offset(base, stack_space, offset, depth - 1);
        }
        case ghidra::CPUI_INT_ADD: {
            int64_t off = 0;
            ghidra::Varnode* lhs = def->getIn(0);
            ghidra::Varnode* rhs = def->getIn(1);
            if (get_signed_offset(lhs, off)) {
                offset += off;
                return resolve_stack_offset(rhs, stack_space, offset, depth - 1);
            }
            if (get_signed_offset(rhs, off)) {
                offset += off;
                return resolve_stack_offset(lhs, stack_space, offset, depth - 1);
            }
            return false;
        }
        case ghidra::CPUI_INT_SUB: {
            int64_t off = 0;
            ghidra::Varnode* lhs = def->getIn(0);
            ghidra::Varnode* rhs = def->getIn(1);
            if (get_signed_offset(rhs, off)) {
                offset -= off;
                return resolve_stack_offset(lhs, stack_space, offset, depth - 1);
            }
            return false;
        }
        case ghidra::CPUI_MULTIEQUAL:
        case ghidra::CPUI_INDIRECT: {
            for (int slot = 0; slot < def->numInput(); ++slot) {
                int64_t candidate = offset;
                if (resolve_stack_offset(def->getIn(slot), stack_space, candidate, depth - 1)) {
                    offset = candidate;
                    return true;
                }
            }
            return false;
        }
        default:
            return false;
    }
}

void StackFrameAnalyzer::collect_stack_accesses(Funcdata* fd) {
    if (!fd) return;
    
    AddrSpace* stack_space = arch->getStackSpace();
    if (!stack_space) return;

    // First pass: collect stack varnodes already mapped to stack space
    for (auto iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn || vn->isAnnotation() || vn->isConstant()) {
            continue;
        }
        if (vn->getSpace() != stack_space) {
            continue;
        }
        int64_t offset = normalize_stack_offset(stack_space, vn->getOffset());
        int size = vn->getSize();
        if (size <= 0) {
            continue;
        }
        auto& entry = stack_accesses[offset];
        entry.first = std::max(entry.first, size);
        entry.second++;

        // Check if this stack varnode holds a pointer return
        // This handles variables that are already mapped to stack space (no longer LOAD/STORE)
        if (size == arch->types->getSizeOfPointer() && vn->isWritten()) {
            Varnode* current = vn;
            int depth = 0;
            while (current && current->isWritten() && depth < 5) {
                PcodeOp* def = current->getDef();
                if (!def) break;
                
                OpCode def_opc = def->code();
                if (def_opc == CPUI_CALL || def_opc == CPUI_CALLIND) {
                    pointer_fields.insert(offset);
                    fission::utils::log_stream() << "[StackFrameAnalyzer] Found pointer stack var at offset " 
                              << offset << " from CALL" << std::endl;
                    break;
                }
                
                if (def_opc == CPUI_CAST || def_opc == CPUI_COPY || 
                    def_opc == CPUI_INT_ZEXT || def_opc == CPUI_INT_SEXT) {
                    current = def->getIn(0);
                    depth++;
                } else {
                    break;
                }
            }
        }
    }
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        OpCode opc = op->code();
        if (opc != CPUI_LOAD && opc != CPUI_STORE) continue;
        
        // Check if accessing stack
        Varnode* addr_vn = op->getIn(1);
        if (!addr_vn) continue;
        
        int64_t offset = 0;
        if (!resolve_stack_offset(addr_vn, stack_space, offset)) {
            continue;
        }

        int size = (opc == CPUI_LOAD) ? op->getOut()->getSize() : op->getIn(2)->getSize();
        
        // Track access
        auto& entry = stack_accesses[offset];
        entry.first = std::max(entry.first, size);
        entry.second++;
        
        // Check if the loaded value is used as a pointer
        if (opc == CPUI_LOAD) {
            Varnode* load_out = op->getOut();
            if (load_out && fission::types::TypeResolver::is_pointer_access(
                    load_out, arch->types->getSizeOfPointer())) {
                pointer_fields.insert(offset);
            }
        }
        
        if (opc == CPUI_STORE) {
            Varnode* val = op->getIn(2);
            if (val) {
                fission::utils::log_stream() << "[StackFrameAnalyzer] STORE at " << offset 
                          << " Size: " << val->getSize() 
                          << " Written: " << val->isWritten() << std::endl;
                if (val->isWritten()) {
                    PcodeOp* d = val->getDef();
                    if (d) fission::utils::log_stream() << "  Def: " << (int)d->code() << std::endl;
                }
            }
            if (val && val->getSize() == arch->types->getSizeOfPointer()) {
                // Check if value comes from a function call (likely pointer return)
                // Trace back through CASTs, COPYs, etc.
                Varnode* current = val;
                int depth = 0;
                while (current && current->isWritten() && depth < 5) {
                    PcodeOp* def = current->getDef();
                    if (!def) break;
                    
                    OpCode def_opc = def->code();
                    if (def_opc == CPUI_CALL || def_opc == CPUI_CALLIND) {
                        pointer_fields.insert(offset);
                        fission::utils::log_stream() << "[StackFrameAnalyzer] Found pointer store at offset " 
                                  << offset << " from CALL (depth " << depth << ")" << std::endl;
                        break;
                    }
                    
                    if (def_opc == CPUI_CAST || def_opc == CPUI_COPY || 
                        def_opc == CPUI_INT_ZEXT || def_opc == CPUI_INT_SEXT) {
                        current = def->getIn(0);
                        depth++;
                    } else {
                        break;
                    }
                }
                // Also check if value is used as pointer elsewhere
                if (fission::types::TypeResolver::is_pointer_access(
                        val, arch->types->getSizeOfPointer())) {
                    pointer_fields.insert(offset);
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
            m.is_pointer = (pointer_fields.count(off) > 0);
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
            m.is_pointer = (pointer_fields.count(off) > 0);
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
    
    // ── Array-synthesis shortcut ──────────────────────────────────────────
    // When every member has the same element size and offsets advance by
    // exactly that size (i.e. the cluster IS an array, not an ad-hoc struct),
    // create a TypeArray instead of a TypeStruct.  This mirrors Ghidra's own
    // analysis for things like `int local[5]` on the stack.
    if (cluster.members.size() >= 2) {
        int elem_sz = cluster.members[0].size;
        bool uniform = true;
        for (size_t mi = 1; mi < cluster.members.size(); ++mi) {
            if (cluster.members[mi].size != elem_sz ||
                cluster.members[mi].offset != (int)(mi * elem_sz)) {
                uniform = false;
                break;
            }
        }
        if (uniform && elem_sz > 0) {
            // Determine element Datatype
            int ptr_size = tf->getSizeOfPointer();
            Datatype* elem_type = nullptr;
            if (cluster.members[0].is_pointer && elem_sz == ptr_size) {
                Datatype* void_type = tf->getTypeVoid();
                elem_type = tf->getTypePointer(ptr_size, void_type, ptr_size);
            } else {
                elem_type = tf->getBase(elem_sz, TYPE_UINT);  // undefined4 / uint
            }
            if (elem_type) {
                int4 count = (int4)cluster.members.size();
                TypeArray* arr = tf->getTypeArray(count, elem_type);
                // TypeArray is not a TypeStruct — callers receive nullptr and
                // fall back to the raw array type via a separate path.
                // For now we record it in the factory and return nullptr to
                // skip the struct-map assignment; the presence of the TypeArray
                // in the factory lets Ghidra's type propagation use it.
                (void)arr;  // registered in TypeFactory; no further action needed
                fission::utils::log_stream() << "[StackFrameAnalyzer] Synthesised array["
                          << count << "] of elem_size=" << elem_sz
                          << " at cluster base_offset=" << cluster.base_offset << std::endl;
                return nullptr;  // caller will skip this cluster
            }
        }
    }
    // ── Standard struct path ─────────────────────────────────────────────
    // Check if already exists
    Datatype* existing = tf->findByName(cluster.inferred_name);
    if (existing) {
        if (existing->getMetatype() == TYPE_STRUCT) {
            return (TypeStruct*)existing;
        }
        return nullptr;
    }
    
    // Create new structure
    TypeStruct* ts = tf->getTypeStruct(cluster.inferred_name);
    
    // Build fields
    std::vector<TypeField> fields;
    int ptr_size = tf->getSizeOfPointer();
    for (const auto& m : cluster.members) {
        Datatype* field_type = m.type;
        if (!field_type) {
            if (m.is_pointer && m.size == ptr_size) {
                // Create void* for pointer fields
                Datatype* void_type = tf->getTypeVoid();
                field_type = tf->getTypePointer(ptr_size, void_type, ptr_size);
            } else {
                // Default to unsigned of appropriate size
                field_type = tf->getBase(m.size, TYPE_UINT);
            }
        }
        fields.push_back(TypeField(0, m.offset, m.name, field_type));
    }
    
    // Set fields
    if (!fields.empty()) {
        tf->setFields(fields, ts, cluster.size, 0, 0);
    }
    
    return ts;
}

std::map<int64_t, TypeStruct*> StackFrameAnalyzer::build_struct_map(TypeFactory* tf) {
    std::map<int64_t, TypeStruct*> result;
    if (!tf) return result;

    for (const auto& cluster : clusters) {
        TypeStruct* ts = create_struct_for_cluster(tf, cluster);
        if (!ts) {
            Datatype* existing = tf->findByName(cluster.inferred_name);
            if (existing && existing->getMetatype() == TYPE_STRUCT) {
                ts = (TypeStruct*)existing;
            }
        }
        if (ts) {
            result[cluster.base_offset] = ts;
        }
    }

    return result;
}

int StackFrameAnalyzer::analyze(Funcdata* fd) {
    clear();
    
    if (!fd) return 0;
    
    collect_stack_accesses(fd);
    cluster_accesses();
    
    fission::utils::log_stream() << "[StackFrameAnalyzer] Found " << stack_accesses.size() 
              << " stack accesses, " << clusters.size() << " structures, "
              << pointer_fields.size() << " pointer fields" << std::endl;
    
    return clusters.size();
}

void StackFrameAnalyzer::apply_structures(TypeFactory* tf) {
    for (const auto& cluster : clusters) {
        bool existed = false;
        if (tf) {
            Datatype* existing = tf->findByName(cluster.inferred_name);
            existed = (existing != nullptr);
        }
        TypeStruct* ts = create_struct_for_cluster(tf, cluster);
        if (ts && !existed) {
            fission::utils::log_stream() << "[StackFrameAnalyzer] Created " << cluster.inferred_name 
                      << " with " << cluster.members.size() << " fields" << std::endl;
        }
    }
}

void StackFrameAnalyzer::clear() {
    stack_accesses.clear();
    pointer_fields.clear();
    clusters.clear();
}

} // namespace analysis
} // namespace fission
