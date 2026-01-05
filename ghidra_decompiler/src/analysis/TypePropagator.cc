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

TypePropagator::TypePropagator(Architecture* a) : arch(a), struct_registry(nullptr) {}

TypePropagator::TypePropagator(Architecture* a, std::map<uint64_t, std::map<int, std::string>>* registry) 
    : arch(a), struct_registry(registry) {}

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
    
    // Get function name for Windows API inference
    std::string func_name = target_func->getName();
    
    // Enhanced type inference for common Windows APIs
    infer_windows_api_types(call_op, func_name);
    
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

void TypePropagator::infer_windows_api_types(PcodeOp* call_op, const std::string& func_name) {
    if (!call_op) return;
    
    TypeFactory* tf = arch->types;
    if (!tf) return;
    
    // Get base integer type for pointer creation
    Datatype* base_int = tf->getBase(1, TYPE_INT);
    if (!base_int) return;
    
    // Handle common patterns
    // CreateFileW(lpFileName, dwDesiredAccess, dwShareMode, lpSecurityAttributes, dwCreationDisposition, dwFlagsAndAttributes, hTemplateFile)
    if (func_name.find("CreateFile") != std::string::npos) {
        if (call_op->numInput() >= 2) {
            Varnode* filename = call_op->getIn(1);
            if (filename) {
                // Try to get wchar_t pointer type
                Datatype* wchar_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                         tf->getBase(2, TYPE_INT), 
                                                         arch->getDefaultCodeSpace()->getWordSize());
                if (wchar_ptr) propagate_backwards(filename, wchar_ptr);
            }
        }
        return;
    }
    
    // WriteFile(hFile, lpBuffer, nNumberOfBytesToWrite, lpNumberOfBytesWritten, lpOverlapped)
    if (func_name.find("WriteFile") != std::string::npos || func_name.find("ReadFile") != std::string::npos) {
        if (call_op->numInput() >= 3) {
            Varnode* buffer = call_op->getIn(2);
            if (buffer) {
                Datatype* void_type = tf->getBase(1, TYPE_VOID);
                if (void_type) {
                    Datatype* void_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                            void_type, 
                                                            arch->getDefaultCodeSpace()->getWordSize());
                    if (void_ptr) propagate_backwards(buffer, void_ptr);
                }
            }
        }
        return;
    }
    
    // sprintf, printf family - first arg is char*
    if (func_name.find("printf") != std::string::npos || func_name.find("sprintf") != std::string::npos) {
        if (call_op->numInput() >= 2) {
            Varnode* format = call_op->getIn(1);
            if (format) {
                Datatype* char_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                        tf->getBase(1, TYPE_INT), 
                                                        arch->getDefaultCodeSpace()->getWordSize());
                if (char_ptr) propagate_backwards(format, char_ptr);
            }
        }
        return;
    }
    
    // malloc/calloc/realloc - returns void*
    if (func_name == "malloc" || func_name == "calloc" || func_name == "realloc") {
        Varnode* output = call_op->getOut();
        if (output) {
            Datatype* void_type = tf->getBase(1, TYPE_VOID);
            if (void_type) {
                Datatype* void_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                        void_type, 
                                                        arch->getDefaultCodeSpace()->getWordSize());
                if (void_ptr) {
                    uint64_t vid = get_varnode_id(output);
                    inferred_types[vid] = void_ptr;
                }
            }
        }
        return;
    }
    
    // strlen/wcslen - arg is string, returns size_t
    if (func_name == "strlen" || func_name == "wcslen") {
        if (call_op->numInput() >= 2) {
            Varnode* str = call_op->getIn(1);
            if (str) {
                bool is_wide = (func_name == "wcslen");
                int char_size = is_wide ? 2 : 1;
                Datatype* str_type = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                        tf->getBase(char_size, TYPE_INT), 
                                                        arch->getDefaultCodeSpace()->getWordSize());
                if (str_type) propagate_backwards(str, str_type);
            }
        }
        return;
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
            // Direct copy - propagate to input
            if (def->numInput() > 0) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        case CPUI_CAST:
            // Cast - propagate to input (may need adjustment)
            if (def->numInput() > 0) {
                Varnode* input = def->getIn(0);
                // For casts, try to infer input type based on output
                if (input->getSize() == type->getSize()) {
                    propagate_backwards(input, type);
                }
            }
            break;
            
        case CPUI_LOAD:
            // Load from memory - could track pointer type
            if (def->numInput() >= 2) {
                Varnode* ptr = def->getIn(1);
                // Create pointer type to loaded value
                Datatype* ptr_type = arch->types->getTypePointer(
                    arch->getDefaultCodeSpace()->getAddrSize(),
                    type,
                    arch->getDefaultCodeSpace()->getWordSize()
                );
                if (ptr_type) {
                    propagate_backwards(ptr, ptr_type);
                }
            }
            break;
            
        case CPUI_MULTIEQUAL:
            // PHI node - propagate to all inputs
            for (int i = 0; i < def->numInput(); ++i) {
                propagate_backwards(def->getIn(i), type);
            }
            break;
            
        case CPUI_INT_ADD:
        case CPUI_INT_SUB:
            // Arithmetic operations - propagate integer type
            if (type->getMetatype() == TYPE_INT || type->getMetatype() == TYPE_UINT) {
                for (int i = 0; i < def->numInput(); ++i) {
                    Varnode* input = def->getIn(i);
                    if (input->getSize() == type->getSize()) {
                        propagate_backwards(input, type);
                    }
                }
            }
            break;
            
        case CPUI_PTRSUB:
        case CPUI_PTRADD:
            // Pointer arithmetic - first input should be pointer
            if (def->numInput() > 0 && type->getMetatype() == TYPE_PTR) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        case CPUI_INT_ZEXT:
        case CPUI_INT_SEXT:
            // Extension operations - propagate smaller type to input
            if (def->numInput() > 0) {
                Varnode* input = def->getIn(0);
                Datatype* input_type = arch->types->getBase(
                    input->getSize(),
                    (opc == CPUI_INT_SEXT) ? TYPE_INT : TYPE_UINT
                );
                if (input_type) {
                    propagate_backwards(input, input_type);
                }
            }
            break;
            
        default:
            // For other operations, don't propagate backwards
            break;
    }
}

bool TypePropagator::propagate_type_edge(PcodeOp* op, int inslot, int outslot) {
    if (!op) return false;
    
    Varnode* vn_in = nullptr;
    Varnode* vn_out = nullptr;
    Datatype* type_in = nullptr;
    Datatype* type_out = nullptr;
    
    // Get input/output varnodes
    if (inslot >= 0 && inslot < op->numInput()) {
        vn_in = op->getIn(inslot);
        if (vn_in) type_in = vn_in->getTempType();
    }
    
    if (outslot == -1) {
        vn_out = op->getOut();
        if (vn_out) type_out = vn_out->getTempType();
    } else if (outslot >= 0 && outslot < op->numInput()) {
        vn_out = op->getIn(outslot);
        if (vn_out) type_out = vn_out->getTempType();
    }
    
    if (!vn_in || !vn_out || !type_in) return false;
    
    OpCode opc = op->code();
    bool changed = false;
    
    switch (opc) {
        case CPUI_COPY:
            // Direct type propagation
            if (outslot == -1 && vn_out && type_out != type_in) {
                vn_out->setTempType(type_in);
                changed = true;
            }
            break;
            
        case CPUI_CAST:
            // Cast may change type but preserve semantic meaning
            if (outslot == -1 && vn_out) {
                if (vn_out->getSize() == vn_in->getSize() && type_out != type_in) {
                    vn_out->setTempType(type_in);
                    changed = true;
                }
            }
            break;
            
        case CPUI_LOAD:
            // Dereference pointer
            if (inslot == 1 && type_in->getMetatype() == TYPE_PTR) {
                TypePointer* ptr_type = (TypePointer*)type_in;
                Datatype* pointed_to = ptr_type->getPtrTo();
                if (vn_out && pointed_to && type_out != pointed_to) {
                    vn_out->setTempType(pointed_to);
                    changed = true;
                }
            }
            break;
            
        case CPUI_STORE:
            // Store to memory - propagate type to stored value
            if (inslot == 1 && type_in->getMetatype() == TYPE_PTR && outslot == 2) {
                TypePointer* ptr_type = (TypePointer*)type_in;
                Datatype* pointed_to = ptr_type->getPtrTo();
                if (vn_out && pointed_to && type_out != pointed_to) {
                    vn_out->setTempType(pointed_to);
                    changed = true;
                }
            }
            break;
            
        case CPUI_MULTIEQUAL:
            // PHI node - propagate most specific type
            if (outslot == -1 && vn_out) {
                Datatype* best_type = type_in;
                for (int i = 0; i < op->numInput(); ++i) {
                    Varnode* input = op->getIn(i);
                    if (!input) continue;
                    Datatype* input_type = input->getTempType();
                    if (input_type && input_type->getSize() > best_type->getSize()) {
                        best_type = input_type;
                    }
                }
                if (type_out != best_type) {
                    vn_out->setTempType(best_type);
                    changed = true;
                }
            }
            break;
            
        case CPUI_PTRSUB:
        case CPUI_PTRADD:
            // Pointer arithmetic preserves pointer type
            if (inslot == 0 && outslot == -1 && vn_out) {
                if (type_in->getMetatype() == TYPE_PTR && type_out != type_in) {
                    vn_out->setTempType(type_in);
                    changed = true;
                }
            }
            break;
            
        default:
            break;
    }
    
    return changed;
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

void TypePropagator::propagate_one_type(Varnode* vn) {
    if (!vn) return;
    
    // Use a work queue for propagation
    std::vector<Varnode*> work_queue;
    work_queue.push_back(vn);
    vn->setMark();
    
    while (!work_queue.empty()) {
        Varnode* current = work_queue.back();
        work_queue.pop_back();
        
        // Propagate to all descendant operations
        list<PcodeOp*>::const_iterator iter;
        for (iter = current->beginDescend(); iter != current->endDescend(); ++iter) {
            PcodeOp* op = *iter;
            if (op->isDead()) continue;
            
            int inslot = op->getSlot(current);
            
            // Try to propagate to output
            if (propagate_type_edge(op, inslot, -1)) {
                Varnode* out = op->getOut();
                if (out && !out->isMark()) {
                    work_queue.push_back(out);
                    out->setMark();
                }
            }
            
            // Try to propagate to other inputs
            for (int outslot = 0; outslot < op->numInput(); ++outslot) {
                if (outslot == inslot) continue;
                if (propagate_type_edge(op, inslot, outslot)) {
                    Varnode* in = op->getIn(outslot);
                    if (in && !in->isMark() && !in->isAnnotation()) {
                        work_queue.push_back(in);
                        in->setMark();
                    }
                }
            }
        }
        
        // Also check definition
        if (current->isWritten()) {
            PcodeOp* def = current->getDef();
            if (def && !def->isDead()) {
                for (int inslot = 0; inslot < def->numInput(); ++inslot) {
                    if (propagate_type_edge(def, -1, inslot)) {
                        Varnode* in = def->getIn(inslot);
                        if (in && !in->isMark() && !in->isAnnotation()) {
                            work_queue.push_back(in);
                            in->setMark();
                        }
                    }
                }
            }
        }
        
        current->clearMark();
    }
}

int TypePropagator::propagate(Funcdata* fd) {
    if (!fd) return 0;
    
    clear();
    int count = 0;
    
    // Phase 1: Find all CALL operations
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (op && op->code() == CPUI_CALL) {
            propagate_from_call(fd, op);
            count++;
        }
    }
    
    // Phase 2: Edge-based propagation for all varnodes with types
    VarnodeLocSet::const_iterator vn_iter;
    for (vn_iter = fd->beginLoc(); vn_iter != fd->endLoc(); ++vn_iter) {
        Varnode* vn = *vn_iter;
        if (vn->isAnnotation()) continue;
        if (!vn->isWritten() && vn->hasNoDescend()) continue;
        
        // Check if this varnode has an inferred type
        uint64_t vid = get_varnode_id(vn);
        if (inferred_types.find(vid) != inferred_types.end()) {
            propagate_one_type(vn);
        }
    }
    
    // Phase 3: Apply inferred types
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

bool TypePropagator::propagate_struct_types(Funcdata* fd) {
    if (!fd || !struct_registry || struct_registry->empty()) return false;
    
    bool changed = false;
    TypeFactory* tf = arch->types;
    if (!tf) return false;
    
    // Scan all CALL operations
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op || op->code() != CPUI_CALL) continue;
        
        Varnode* target = op->getIn(0);
        if (!target || !target->isConstant()) continue;
        
        uint64_t callee_addr = target->getOffset();
        
        // Check if this function has registered struct parameters
        if (struct_registry->count(callee_addr)) {
            const auto& params_map = struct_registry->at(callee_addr);
            
            // Apply struct types to each argument
            int num_inputs = op->numInput();
            for (int i = 1; i < num_inputs; ++i) {
                int param_index = i - 1;
                if (params_map.count(param_index)) {
                    std::string struct_name = params_map.at(param_index);
                    
                    // Find struct type by name
                    Datatype* type = tf->findByName(struct_name);
                    if (type) {
                        // Create pointer to struct
                        Datatype* ptr_type = tf->getTypePointer(8, type, 0);
                        
                        Varnode* arg = op->getIn(i);
                        if (arg) {
                            // Force update with type lock
                            arg->updateType(ptr_type, true, true);
                            changed = true;
                            
                            std::cerr << "[TypePropagator] Applied " << struct_name 
                                      << "* to arg " << i << " of call to 0x" 
                                      << std::hex << callee_addr << std::dec << std::endl;
                        }
                    }
                }
            }
        }
    }
    
    return changed;
}

std::string TypePropagator::apply_struct_types(
    std::string c_code,
    Funcdata* fd,
    const std::map<unsigned long long, TypeStruct*>& structs
) {
    if (!fd || structs.empty()) return c_code;

    const FuncProto& proto = fd->getFuncProto();
    int numParams = proto.numParams();
    
    for (int i = 0; i < numParams; ++i) {
        ProtoParameter* param = proto.getParam(i);
        if (!param) continue;
        
        uint64_t off = param->getAddress().getOffset();
        
        if (structs.count(off)) {
            TypeStruct* st = structs.at(off);
            if (!st) continue;
            
            std::string sname = st->getName();
            std::string pname = param->getName();
            
            // Search for pointer declaration: "*pname" or "* pname"
            std::string target = "*" + pname;
            size_t pos = c_code.find(target);
            
            if (pos == std::string::npos) {
                target = "* " + pname;
                pos = c_code.find(target);
            }
            
            if (pos != std::string::npos) {
                // Backtrack to find type name start
                size_t type_end = pos;
                while (type_end > 0 && (c_code[type_end-1] == ' ' || c_code[type_end-1] == '\t')) {
                    type_end--;
                }
                
                size_t type_start = type_end;
                while (type_start > 0) {
                    char c = c_code[type_start-1];
                    if (c == ' ' || c == '\t' || c == '\n' || c == '(' || c == ',') break;
                    type_start--;
                }
                
                if (type_start < type_end) {
                    std::string old_type = c_code.substr(type_start, type_end - type_start);
                    c_code.replace(type_start, type_end - type_start, sname);
                    
                    std::cerr << "[TypePropagator] Replaced type '" << old_type 
                              << "' for " << pname << " with " << sname << std::endl;
                }
            }
        }
    }
    
    return c_code;
}

std::string TypePropagator::get_fid_filename(bool is_64bit, const std::string& compiler_id) {
    std::string suffix = is_64bit ? "_x64.fidbf" : "_x86.fidbf";
    std::string fid_filename = "vs2019" + suffix; // Default

    if (compiler_id.find("vs2017") != std::string::npos) 
        fid_filename = "vs2017" + suffix;
    else if (compiler_id.find("vs2015") != std::string::npos) 
        fid_filename = "vs2015" + suffix;
    else if (compiler_id.find("vs2012") != std::string::npos) 
        fid_filename = "vs2012" + suffix;
    
    return fid_filename;
}

} // namespace analysis
} // namespace fission
