#include "fission/decompiler/TypeEnhancer.h"
#include "op.hh"
#include <iostream>
#include <iomanip>

// External global registry from fission_decomp.cpp
extern std::map<uint64_t, std::map<int, std::string>> global_struct_registry;

namespace fission {
namespace decompiler {

bool TypeEnhancer::propagate_reverse_types(ghidra::Funcdata* fd, ghidra::TypeFactory* type_factory) {
    bool changed = false;
    if (!fd) return false;
    
    auto iter = fd->beginOpAll();
    auto end_iter = fd->endOpAll();
    
    for (; iter != end_iter; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op || op->isDead()) continue;
        
        if (op->code() == ghidra::CPUI_CALL) {
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                uint64_t callee_addr = target->getOffset();
                
                if (global_struct_registry.count(callee_addr)) {
                    const auto& params_map = global_struct_registry[callee_addr];
                    
                    // Iterate arguments (Input 1+)
                    int num_inputs = op->numInput();
                    for (int i=1; i<num_inputs; ++i) {
                        int param_index = i - 1; // 0-based param index
                        if (params_map.count(param_index)) {
                            std::string struct_name = params_map.at(param_index);
                            
                            // Find type by name
                            ghidra::Datatype* type = type_factory->findByName(struct_name);
                            if (type) {
                                // Create pointer to struct
                                ghidra::Datatype* ptr_type = type_factory->getTypePointer(8, type, 0); // 8-byte ptr
                                
                                ghidra::Varnode* arg = op->getIn(i);
                                
                                // FORCE update the argument type!
                                // We use lock=true to ensure it sticks.
                                if (arg) {
                                    arg->updateType(ptr_type, true, true);
                                    changed = true;
                                    std::cerr << "[ReverseProp] Applied " << struct_name << "* to arg " << i 
                                             << " of call to 0x" << std::hex << callee_addr << std::dec << std::endl;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    return changed;
}

std::string TypeEnhancer::get_fid_filename(bool is_64bit, const std::string& compiler_id) {
    std::string suffix = is_64bit ? "_x64.fidbf" : "_x86.fidbf";
    std::string fid_filename = "vs2019" + suffix; // Default

    if (compiler_id.find("vs2017") != std::string::npos) fid_filename = "vs2017" + suffix;
    else if (compiler_id.find("vs2015") != std::string::npos) fid_filename = "vs2015" + suffix;
    else if (compiler_id.find("vs2012") != std::string::npos) fid_filename = "vs2012" + suffix;
    
    return fid_filename;
}

std::string TypeEnhancer::apply_struct_types(
    std::string c_code, 
    ghidra::Funcdata* fd,
    const std::map<unsigned long long, ghidra::TypeStruct*>& structs
) {
    if (!fd || structs.empty()) return c_code;

    const ghidra::FuncProto& proto = fd->getFuncProto();
    int numParams = proto.numParams();
    
    for(int i=0; i<numParams; ++i) {
        ghidra::ProtoParameter* param = proto.getParam(i);
        if (!param) continue;
        
        // Match by storage offset
        // Note: Varnode/Param addresses are offsets in their space.
        // inferred_structs uses offset from Varnode::getOffset()
        uint64_t off = param->getAddress().getOffset();
        
        if (structs.count(off)) {
            ghidra::TypeStruct* st = structs.at(off);
            if (!st) continue;
            
            std::string sname = st->getName();
            std::string pname = param->getName(); // e.g., "param_1"
            
            // Search for pointer declaration: "*pname" or "* pname"
            // We want to replace the type preceding it.
            // Example: "DWORD *param_1" -> "f_... *param_1"
            
            std::string target = "*" + pname;
            size_t pos = c_code.find(target);
            
            // Try with space if not found
            if (pos == std::string::npos) {
                target = "* " + pname;
                pos = c_code.find(target);
            }
            
            if (pos != std::string::npos) {
                // Found declaration. Now backtrack to find the Type name start.
                // Skip spaces backwards from '*'
                size_t type_end = pos;
                while (type_end > 0 && (c_code[type_end-1] == ' ' || c_code[type_end-1] == '\t')) {
                    type_end--;
                }
                
                // Now backwards to find start of token
                size_t type_start = type_end;
                while (type_start > 0) {
                    char c = c_code[type_start-1];
                    if (c == ' ' || c == '\t' || c == '\n' || c == '(' || c == ',') break;
                    type_start--;
                }
                
                if (type_start < type_end) {
                    // Replace the type word with struct name
                    std::string old_type = c_code.substr(type_start, type_end - type_start);
                    // Avoid replacing "void" or "return" accidentally, but current logic is tight to "*pname"
                    c_code.replace(type_start, type_end - type_start, sname);
                    std::cerr << "[apply_struct_types] Replaced type '" << old_type 
                             << "' for " << pname << " with " << sname << std::endl;
                }
            }
        }
    }
    return c_code;
}

} // namespace decompiler
} // namespace fission
