/**
 * Fission Decompiler - Type Manager
 * Manages Ghidra type registration and GDT resolution
 */

#ifndef FISSION_TYPES_TYPE_MANAGER_H
#define FISSION_TYPES_TYPE_MANAGER_H

#include <string>
#include <map>
#include <vector>
#include "type.hh"
#include "fission/types/GdtParser.h"

namespace fission {
namespace types {

using namespace ghidra;

class TypeManager {
public:
    /**
     * Resolve GDT C-style type string to Ghidra Datatype
     */
    static Datatype* resolve_gdt_type(TypeFactory* types, const std::string& type_name, 
                                      int size, const GdtData& gdt_data, int ptr_size);
    
    /**
     * Load all types from GDT data into Ghidra TypeFactory
     */
    static void load_gdt_types(TypeFactory* types, const GdtData& gdt_data, int ptr_size);
    
    /**
     * Register standard Windows types (BYTE, DWORD, HANDLE, etc.)
     */
    static void register_windows_types(TypeFactory* types, int ptr_size);
};

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_TYPE_MANAGER_H
