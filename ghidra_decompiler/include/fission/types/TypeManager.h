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
#include "fission/types/GdtBinaryParser.h"

namespace fission {
namespace types {

using namespace ghidra;

class TypeManager {
public:
    /**
     * Load all types from GdtBinaryParser into Ghidra TypeFactory
     */
    static void load_types_from_gdt(TypeFactory* types, const GdtBinaryParser* gdt, int ptr_size);
    
    /**
     * Register standard Windows types (BYTE, DWORD, HANDLE, etc.)
     */
    static void register_windows_types(TypeFactory* types, int ptr_size);
};

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_TYPE_MANAGER_H
