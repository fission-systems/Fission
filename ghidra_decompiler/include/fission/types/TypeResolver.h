#ifndef __TYPE_RESOLVER_H__
#define __TYPE_RESOLVER_H__

#include <map>
#include <set>
#include <string>

// Forward declarations
namespace ghidra {
    class PcodeOp;
    class Varnode;
    class Funcdata;
    class TypeFactory;
    class Datatype;
}

namespace fission {
namespace types {

/**
 * TypeResolver - Precise Field Type Detection
 * 
 * Enhances structure field typing by:
 * 1. Detecting float/double types via FPU/XMM register usage
 * 2. Identifying pointer fields that reference other structures
 * 3. Providing recursive type resolution for nested structures
 */
class TypeResolver {
public:
    /**
     * Checks if a PcodeOp involves floating-point operations.
     * This is determined by checking for FLOAT_* opcodes.
     */
    static bool is_float_operation(ghidra::PcodeOp* op);
    
    /**
     * Checks if a Varnode is used in floating-point context.
     * Traces descendants to find FLOAT operations.
     */
    static bool is_used_as_float(ghidra::Varnode* vn);
    
    /**
     * Given a memory access size, determine if it's likely a pointer.
     * Returns true if size matches pointer size AND the value is used
     * to dereference memory.
     */
    static bool is_pointer_access(ghidra::Varnode* value_vn, int ptr_size);
    
    /**
     * Get the appropriate Ghidra type for a field based on analysis.
     * 
     * @param factory TypeFactory for creating/retrieving types
     * @param size Size of the field in bytes
     * @param is_float True if field is used in float operations
     * @param is_pointer True if field is used as a pointer
     * @param ptr_size Pointer size for the architecture
     * @return Best-matching Datatype
     */
    static ghidra::Datatype* get_field_type(
        ghidra::TypeFactory* factory,
        int size,
        bool is_float,
        bool is_pointer,
        int ptr_size
    );

    /**
     * Analyze all uses of a value to determine its most likely type.
     * This is a comprehensive analysis that checks:
     * - Is it used in float operations?
     * - Is it used as a pointer (dereferenced)?
     * - Is it passed to known API functions with typed parameters?
     */
    struct TypeHint {
        bool is_float = false;
        bool is_pointer = false;
        bool is_signed = true; // default assumption
        std::string suggested_name; // e.g., "float", "ptr", "int"
    };
    
    static TypeHint analyze_value_usage(ghidra::Varnode* vn, int ptr_size);
};

} // namespace types
} // namespace fission

#endif
