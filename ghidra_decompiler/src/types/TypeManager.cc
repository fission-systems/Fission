/**
 * Fission Decompiler - Type Manager Implementation
 */

#include "fission/types/TypeManager.h"
#include <iostream>
#include <algorithm>

namespace fission {
namespace types {

// Ghidra core types
static std::map<std::string, std::pair<int, type_metatype>> PRIMITIVE_MAP = {
    {"void", {0, TYPE_VOID}},
    {"bool", {1, TYPE_BOOL}},
    {"char", {1, TYPE_INT}},
    {"unsigned char", {1, TYPE_UINT}},
    {"short", {2, TYPE_INT}},
    {"unsigned short", {2, TYPE_UINT}},
    {"int", {4, TYPE_INT}},
    {"unsigned int", {4, TYPE_UINT}},
    {"long", {4, TYPE_INT}},
    {"unsigned long", {4, TYPE_UINT}},
    {"long long", {8, TYPE_INT}},
    {"unsigned long long", {8, TYPE_UINT}},
    {"__int64", {8, TYPE_INT}},
    {"float", {4, TYPE_FLOAT}},
    {"double", {8, TYPE_FLOAT}}
};

Datatype* TypeManager::resolve_type(TypeFactory* types, const std::string& type_name, 
                                    int size, const GdtBinaryParser* gdt, int ptr_size) {
    // 1. Check if it's a pointer type first (contains '*')
    if (type_name.find('*') != std::string::npos) {
        size_t star_pos = type_name.rfind('*');
        std::string base_name = type_name.substr(0, star_pos);
        while (!base_name.empty() && base_name.back() == ' ') base_name.pop_back();
        
        Datatype* base_type = resolve_type(types, base_name, 0, gdt, ptr_size);
        if (base_type) {
            return types->getTypePointer(size > 0 ? size : ptr_size, base_type, 1);
        }
        return types->getTypePointer(size > 0 ? size : ptr_size, types->getTypeVoid(), 1);
    }
    
    // 2. Check Primitive Map
    auto it = PRIMITIVE_MAP.find(type_name);
    if (it != PRIMITIVE_MAP.end()) {
        if (it->second.second == TYPE_VOID) return types->getTypeVoid();
        return types->getBase(it->second.first, it->second.second);
    }

    // 3. Check registered types
    Datatype* existing = types->findByName(type_name);
    if (existing) return existing;
    
    // 4. Check GDT types if available
    if (gdt) {
        const GdtDataType* gdt_type = gdt->find_type(type_name);
        if (gdt_type) {
            // Create typedef for Windows types
            Datatype* base = types->getBase(gdt_type->size, TYPE_UINT);
            return types->getTypedef(base, type_name, 0, 0);
        }
    }
    
    // Fallback
    return nullptr;
}

void TypeManager::load_types_from_gdt(TypeFactory* types, const GdtBinaryParser* gdt, int ptr_size) {
    if (!gdt || !gdt->is_loaded()) return;
    
    // Register all types from GDT
    for (const auto& [name, dt] : gdt->get_types()) {
        if (!types->findByName(name)) {
            Datatype* base = types->getBase(dt.size, TYPE_UINT);
            if (base) {
                types->getTypedef(base, name, 0, 0);
            }
        }
    }
    
    std::cerr << "[TypeManager] Loaded " << gdt->get_types().size() << " types from GDT" << std::endl;
}

void TypeManager::register_windows_types(TypeFactory* types, int ptr_size) {
    // Pointers
    Datatype* void_t = types->getTypeVoid();
    Datatype* void_ptr = types->getTypePointer(ptr_size, void_t, 1);
    
    types->getTypedef(void_ptr, "LPVOID", 0, 0);
    types->getTypedef(void_ptr, "PVOID", 0, 0);
    types->getTypedef(void_ptr, "HANDLE", 0, 0);
    types->getTypedef(void_ptr, "HWND", 0, 0);
    types->getTypedef(void_ptr, "HINSTANCE", 0, 0);
    types->getTypedef(void_ptr, "HMODULE", 0, 0);
    
    // Strings
    Datatype* char_t = types->getBase(1, TYPE_INT);
    Datatype* char_ptr = types->getTypePointer(ptr_size, char_t, 1);
    types->getTypedef(char_ptr, "LPSTR", 0, 0);
    types->getTypedef(char_ptr, "LPCSTR", 0, 0);
    
    Datatype* wide_char_t = types->getBase(2, TYPE_INT);
    Datatype* wchar_ptr = types->getTypePointer(ptr_size, wide_char_t, 1);
    types->getTypedef(wchar_ptr, "LPWSTR", 0, 0);
    types->getTypedef(wchar_ptr, "LPCWSTR", 0, 0);
    
    // Basic types
    types->getTypedef(types->getBase(1, TYPE_UINT), "BYTE", 0, 0);
    types->getTypedef(types->getBase(2, TYPE_UINT), "WORD", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "DWORD", 0, 0);
    types->getTypedef(types->getBase(8, TYPE_UINT), "QWORD", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_INT), "LONG", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "ULONG", 0, 0);
}

} // namespace types
} // namespace fission
