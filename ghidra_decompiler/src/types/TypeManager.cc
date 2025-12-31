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

Datatype* TypeManager::resolve_gdt_type(TypeFactory* types, const std::string& type_name, 
                                        int size, const GdtData& gdt_data, int ptr_size) {
    // 1. Check if it's a pointer type first (contains '*')
    if (type_name.find('*') != std::string::npos) {
        // Simple pointer case: "char *" -> Pointer(char)
        size_t star_pos = type_name.rfind('*');
        std::string base_name = type_name.substr(0, star_pos);
        // Trim whitespace
        while (!base_name.empty() && base_name.back() == ' ') base_name.pop_back();
        
        Datatype* base_type = resolve_gdt_type(types, base_name, 0, gdt_data, ptr_size);
        if (base_type) {
            return types->getTypePointer(size > 0 ? size : ptr_size, base_type, 1);
        }
        // Fallback to void* if base not found
        return types->getTypePointer(size > 0 ? size : ptr_size, types->getTypeVoid(), 1);
    }
    
    // 2. Check Primitive Map
    auto it = PRIMITIVE_MAP.find(type_name);
    if (it != PRIMITIVE_MAP.end()) {
        if (it->second.second == TYPE_VOID) return types->getTypeVoid();
        return types->getBase(it->second.first, it->second.second);
    }

    // 3. Check registered types (including typedefs and user structs)
    Datatype* existing = types->findByName(type_name);
    if (existing) return existing;
    
    // 4. Check GDT Typedefs
    for (const auto& td : gdt_data.typedefs) {
        if (td.alias == type_name) {
            // Found typedef alias, resolve base
            Datatype* base = resolve_gdt_type(types, td.base, size, gdt_data, ptr_size);
            if (base) {
                // Register typedef
                return types->getTypedef(base, type_name, 0, 0);
            }
        }
    }
    
    // 5. Check GDT Structs (forward declaration if needed)
    for (const auto& s : gdt_data.structs) {
        if (s.name == type_name) {
            // Create empty struct first to handle recursion
            TypeStruct* new_struct = types->getTypeStruct(s.name);
            // Fields will be filled by load_gdt_types in a second pass
            if (s.size > 0) {
                 // Force strict sizing if known? For now just return the type.
            }
            return new_struct;
        }
    }
    
    // Fallback
    return nullptr;
}

void TypeManager::load_gdt_types(TypeFactory* types, const GdtData& gdt_data, int ptr_size) {
    // Pass 1: Create all structs (forward declarations)
    for (const auto& s : gdt_data.structs) {
        if (!types->findByName(s.name)) {
            types->getTypeStruct(s.name);
        }
    }
    
    // Pass 2: Fill struct fields
    for (const auto& s : gdt_data.structs) {
        TypeStruct* str_type = (TypeStruct*)types->findByName(s.name);
        if (!str_type || str_type->getMetatype() != TYPE_STRUCT) continue;
        
        std::vector<TypeField> fields;
        for (const auto& f : s.fields) {
            Datatype* field_type = resolve_gdt_type(types, f.name, f.size, gdt_data, ptr_size); // Name here is type name in GDT usually?
            // Actually GDT field: name="field_name", but type info is implicit or missing in this simplified JSON?
            // Wait, previous code used 'f.name' as type name? Let's check original.
            // Original code: resolve_gdt_type(types, f.name...
            // Ah, GdtField struct in original: name is FIELD NAME. Type is missing?
            // Let's re-read original GdtStruct definition in fission_decomp.cpp
            
            // Correction: In original code, GdtField has name, offset, size. No type string?
            // This suggests the current GDT JSON format is incomplete for full reconstruction OR 
            // the name field actually contains the type?
            
            // Checking original load_gdt_types loop:
            /*
             for (const auto& f : s.fields) {
                 // We don't have field type info in simple GDT!
                 // Assuming undefined bytes for now which is safer than guessing
                 fields.push_back(TypeField(f.offset, f.name, types->getBase(f.size, TYPE_UNKNOWN)));
             }
             */
             
             // Ah, so original code used TYPE_UNKNOWN.
             if (f.size > 0) {
                 fields.push_back(TypeField(0, f.offset, f.name, types->getBase(f.size, TYPE_UNKNOWN)));
             }
        }
        
        // Only set fields if not empty to avoid errors
        if (!fields.empty()) {
            types->setFields(fields, str_type, s.size, s.alignment, 0);
        }
    }
    
    // Pass 3: Register typedefs
    for (const auto& td : gdt_data.typedefs) {
        if (!types->findByName(td.alias)) {
            Datatype* base = resolve_gdt_type(types, td.base, 0, gdt_data, ptr_size);
            if (base) {
                types->getTypedef(base, td.alias, 0, 0);
            }
        }
    }
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
    Datatype* char_t = types->getBase(1, TYPE_INT); // char
    Datatype* char_ptr = types->getTypePointer(ptr_size, char_t, 1);
    types->getTypedef(char_ptr, "LPSTR", 0, 0);
    types->getTypedef(char_ptr, "LPCSTR", 0, 0);
    
    Datatype* wide_char_t = types->getBase(2, TYPE_INT); // wchar_t
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
