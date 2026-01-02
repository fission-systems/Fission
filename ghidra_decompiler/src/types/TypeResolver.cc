#include "fission/types/TypeResolver.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"

namespace fission {
namespace types {

bool TypeResolver::is_float_operation(ghidra::PcodeOp* op) {
    if (!op) return false;
    
    ghidra::OpCode code = op->code();
    
    // All floating-point opcodes in Ghidra
    switch (code) {
        case ghidra::CPUI_FLOAT_ADD:
        case ghidra::CPUI_FLOAT_SUB:
        case ghidra::CPUI_FLOAT_MULT:
        case ghidra::CPUI_FLOAT_DIV:
        case ghidra::CPUI_FLOAT_NEG:
        case ghidra::CPUI_FLOAT_ABS:
        case ghidra::CPUI_FLOAT_SQRT:
        case ghidra::CPUI_FLOAT_CEIL:
        case ghidra::CPUI_FLOAT_FLOOR:
        case ghidra::CPUI_FLOAT_ROUND:
        case ghidra::CPUI_FLOAT_NAN:
        case ghidra::CPUI_FLOAT_EQUAL:
        case ghidra::CPUI_FLOAT_NOTEQUAL:
        case ghidra::CPUI_FLOAT_LESS:
        case ghidra::CPUI_FLOAT_LESSEQUAL:
        case ghidra::CPUI_FLOAT_INT2FLOAT:
        case ghidra::CPUI_FLOAT_FLOAT2FLOAT:
        case ghidra::CPUI_FLOAT_TRUNC:
            return true;
        default:
            return false;
    }
}

bool TypeResolver::is_used_as_float(ghidra::Varnode* vn) {
    if (!vn) return false;
    
    // Check all descendants (uses of this varnode)
    auto iter = vn->beginDescend();
    auto end = vn->endDescend();
    
    for (; iter != end; ++iter) {
        ghidra::PcodeOp* use_op = *iter;
        if (is_float_operation(use_op)) {
            return true;
        }
        
        // Also check if output of use_op is used as float (one level deep)
        ghidra::Varnode* out = use_op->getOut();
        if (out) {
            auto out_iter = out->beginDescend();
            auto out_end = out->endDescend();
            for (; out_iter != out_end; ++out_iter) {
                if (is_float_operation(*out_iter)) {
                    return true;
                }
            }
        }
    }
    
    return false;
}

bool TypeResolver::is_pointer_access(ghidra::Varnode* value_vn, int ptr_size) {
    if (!value_vn) return false;
    
    // Size must match pointer size
    if ((int)value_vn->getSize() != ptr_size) return false;
    
    // Check if this value is used to dereference memory (LOAD/STORE)
    auto iter = value_vn->beginDescend();
    auto end = value_vn->endDescend();
    
    for (; iter != end; ++iter) {
        ghidra::PcodeOp* use_op = *iter;
        if (!use_op) continue;
        
        ghidra::OpCode code = use_op->code();
        
        // LOAD(space, ptr) - ptr is input(1)
        if (code == ghidra::CPUI_LOAD && use_op->getIn(1) == value_vn) {
            return true;
        }
        
        // STORE(space, ptr, val) - ptr is input(1)
        if (code == ghidra::CPUI_STORE && use_op->getIn(1) == value_vn) {
            return true;
        }
        
        // CALL - first argument often is 'this' pointer
        if (code == ghidra::CPUI_CALL) {
            return true;
        }
        
        // PTRSUB/PTRADD operations indicate pointer arithmetic
        if (code == ghidra::CPUI_PTRSUB || code == ghidra::CPUI_PTRADD) {
            return true;
        }
    }
    
    return false;
}

ghidra::Datatype* TypeResolver::get_field_type(
    ghidra::TypeFactory* factory,
    int size,
    bool is_float,
    bool is_pointer,
    int ptr_size
) {
    if (!factory) return nullptr;
    
    // Priority: pointer > float > int
    
    if (is_pointer && size == ptr_size) {
        // Return void* as a generic pointer
        ghidra::Datatype* void_type = factory->getTypeVoid();
        return factory->getTypePointer(ptr_size, void_type, (unsigned int)ptr_size);
    }
    
    if (is_float) {
        if (size == 4) {
            return factory->getBase(4, ghidra::TYPE_FLOAT);
        } else if (size == 8) {
            return factory->getBase(8, ghidra::TYPE_FLOAT);
        }
    }
    
    // Default: integer types
    switch (size) {
        case 1: return factory->getBase(1, ghidra::TYPE_INT);
        case 2: return factory->getBase(2, ghidra::TYPE_INT);
        case 4: return factory->getBase(4, ghidra::TYPE_INT);
        case 8: return factory->getBase(8, ghidra::TYPE_INT);
        default:
            return factory->getBase(size, ghidra::TYPE_UNKNOWN);
    }
}

TypeResolver::TypeHint TypeResolver::analyze_value_usage(ghidra::Varnode* vn, int ptr_size) {
    TypeHint hint;
    
    if (!vn) return hint;
    
    // Check for float usage
    hint.is_float = is_used_as_float(vn);
    
    // Check for pointer usage
    hint.is_pointer = is_pointer_access(vn, ptr_size);
    
    // Determine suggested name
    if (hint.is_float) {
        hint.suggested_name = (vn->getSize() == 4) ? "float" : "double";
    } else if (hint.is_pointer) {
        hint.suggested_name = "ptr";
    } else {
        int size = vn->getSize();
        switch (size) {
            case 1: hint.suggested_name = "byte"; break;
            case 2: hint.suggested_name = "short"; break;
            case 4: hint.suggested_name = "int"; break;
            case 8: hint.suggested_name = "long"; break;
            default: hint.suggested_name = "data"; break;
        }
    }
    
    return hint;
}

} // namespace types
} // namespace fission
