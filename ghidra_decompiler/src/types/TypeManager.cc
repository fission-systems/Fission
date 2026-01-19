/**
 * Fission Decompiler - Type Manager Implementation
 */

#include "fission/types/TypeManager.h"
#include <iostream>
#include "fission/utils/logger.h"
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
    
    fission::utils::log_stream() << "[TypeManager] Loaded " << gdt->get_types().size() << " types from GDT" << std::endl;
}

void TypeManager::register_windows_types(TypeFactory* types, int ptr_size) {
    auto rename_core_type = [&](int size, type_metatype meta, const char* name) {
        if (!types) return;
        if (types->findByName(name)) return;
        Datatype* base = types->getBase(size, meta);
        if (!base) return;
        if (base->getName() == name) return;
        types->setName(base, name);
    };

    rename_core_type(8, TYPE_INT, "longlong");
    rename_core_type(8, TYPE_UINT, "ulonglong");

    // Pointers
    Datatype* void_t = types->getTypeVoid();
    Datatype* void_ptr = types->getTypePointer(ptr_size, void_t, 1);
    
    types->getTypedef(void_ptr, "LPVOID", 0, 0);
    types->getTypedef(void_ptr, "PVOID", 0, 0);
    types->getTypedef(void_ptr, "HANDLE", 0, 0);
    types->getTypedef(void_ptr, "HWND", 0, 0);
    types->getTypedef(void_ptr, "HINSTANCE", 0, 0);
    types->getTypedef(void_ptr, "HMODULE", 0, 0);
    types->getTypedef(void_ptr, "HKEY", 0, 0);
    types->getTypedef(void_ptr, "HGLOBAL", 0, 0);
    types->getTypedef(void_ptr, "HLOCAL", 0, 0);
    types->getTypedef(void_ptr, "HDC", 0, 0);
    types->getTypedef(void_ptr, "HBITMAP", 0, 0);
    types->getTypedef(void_ptr, "HBRUSH", 0, 0);
    types->getTypedef(void_ptr, "HFONT", 0, 0);
    types->getTypedef(void_ptr, "HICON", 0, 0);
    types->getTypedef(void_ptr, "HCURSOR", 0, 0);
    types->getTypedef(void_ptr, "HMENU", 0, 0);
    types->getTypedef(void_ptr, "HRGN", 0, 0);
    types->getTypedef(void_ptr, "HPEN", 0, 0);
    types->getTypedef(void_ptr, "HPALETTE", 0, 0);
    types->getTypedef(void_ptr, "HRSRC", 0, 0);
    types->getTypedef(void_ptr, "HTASK", 0, 0);
    types->getTypedef(void_ptr, "HGDIOBJ", 0, 0);
    types->getTypedef(void_ptr, "HMETAFILE", 0, 0);
    types->getTypedef(void_ptr, "HENHMETAFILE", 0, 0);
    types->getTypedef(void_ptr, "HWINEVENTHOOK", 0, 0);
    types->getTypedef(void_ptr, "HHOOK", 0, 0);
    types->getTypedef(void_ptr, "SC_HANDLE", 0, 0);
    types->getTypedef(void_ptr, "SERVICE_STATUS_HANDLE", 0, 0);
    
    // Strings
    Datatype* char_t = types->getBase(1, TYPE_INT);
    Datatype* char_ptr = types->getTypePointer(ptr_size, char_t, 1);
    types->getTypedef(char_ptr, "LPSTR", 0, 0);
    types->getTypedef(char_ptr, "LPCSTR", 0, 0);
    types->getTypedef(char_ptr, "PSTR", 0, 0);
    types->getTypedef(char_ptr, "PCSTR", 0, 0);
    
    Datatype* wide_char_t = types->getBase(2, TYPE_INT);
    Datatype* wchar_ptr = types->getTypePointer(ptr_size, wide_char_t, 1);
    types->getTypedef(wchar_ptr, "LPWSTR", 0, 0);
    types->getTypedef(wchar_ptr, "LPCWSTR", 0, 0);
    types->getTypedef(wchar_ptr, "PWSTR", 0, 0);
    types->getTypedef(wchar_ptr, "PCWSTR", 0, 0);
    types->getTypedef(wide_char_t, "WCHAR", 0, 0);
    
    // Basic integer types
    types->getTypedef(types->getBase(1, TYPE_UINT), "BYTE", 0, 0);
    types->getTypedef(types->getBase(2, TYPE_UINT), "WORD", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "DWORD", 0, 0);
    types->getTypedef(types->getBase(8, TYPE_UINT), "QWORD", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_INT), "LONG", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "ULONG", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_INT), "INT", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "UINT", 0, 0);
    types->getTypedef(types->getBase(2, TYPE_INT), "SHORT", 0, 0);
    types->getTypedef(types->getBase(2, TYPE_UINT), "USHORT", 0, 0);
    types->getTypedef(types->getBase(1, TYPE_INT), "CHAR", 0, 0);
    types->getTypedef(types->getBase(1, TYPE_UINT), "UCHAR", 0, 0);
    types->getTypedef(types->getBase(1, TYPE_BOOL), "BOOL", 0, 0);
    types->getTypedef(types->getBase(1, TYPE_BOOL), "BOOLEAN", 0, 0);
    
    // Pointer-sized types
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "SIZE_T", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_INT), "SSIZE_T", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "ULONG_PTR", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_INT), "LONG_PTR", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "DWORD_PTR", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_INT), "INT_PTR", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "UINT_PTR", 0, 0);
    
    // HRESULT and NTSTATUS
    types->getTypedef(types->getBase(4, TYPE_INT), "HRESULT", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_INT), "NTSTATUS", 0, 0);
    
    // LARGE_INTEGER (8 bytes)
    types->getTypedef(types->getBase(8, TYPE_INT), "LARGE_INTEGER", 0, 0);
    types->getTypedef(types->getBase(8, TYPE_UINT), "ULARGE_INTEGER", 0, 0);
    
    // LRESULT, WPARAM, LPARAM (pointer-sized)
    types->getTypedef(types->getBase(ptr_size, TYPE_INT), "LRESULT", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "WPARAM", 0, 0);
    types->getTypedef(types->getBase(ptr_size, TYPE_INT), "LPARAM", 0, 0);
    
    // COM interface types
    types->getTypedef(void_ptr, "LPUNKNOWN", 0, 0);
    types->getTypedef(void_ptr, "IUnknown*", 0, 0);
    
    // Security types
    types->getTypedef(void_ptr, "PSID", 0, 0);
    types->getTypedef(void_ptr, "PACL", 0, 0);
    types->getTypedef(void_ptr, "PSECURITY_DESCRIPTOR", 0, 0);
    
    // SEH types (important for exception handling patterns)
    types->getTypedef(void_ptr, "PEXCEPTION_RECORD", 0, 0);
    types->getTypedef(void_ptr, "PCONTEXT", 0, 0);
    types->getTypedef(void_ptr, "EXCEPTION_POINTERS*", 0, 0);
    
    // Thread/Process types
    types->getTypedef(types->getBase(4, TYPE_UINT), "DWORD_THID", 0, 0);
    types->getTypedef(types->getBase(4, TYPE_UINT), "DWORD_PRID", 0, 0);
    
    // Socket types
    types->getTypedef(types->getBase(ptr_size, TYPE_UINT), "SOCKET", 0, 0);
    
    // Function pointer types (generic)
    types->getTypedef(void_ptr, "FARPROC", 0, 0);
    types->getTypedef(void_ptr, "NEARPROC", 0, 0);
    types->getTypedef(void_ptr, "PROC", 0, 0);
    types->getTypedef(void_ptr, "WNDPROC", 0, 0);
    types->getTypedef(void_ptr, "DLGPROC", 0, 0);
    types->getTypedef(void_ptr, "TIMERPROC", 0, 0);
    types->getTypedef(void_ptr, "WNDENUMPROC", 0, 0);
    types->getTypedef(void_ptr, "HOOKPROC", 0, 0);
    types->getTypedef(void_ptr, "LPTHREAD_START_ROUTINE", 0, 0);
    
    // Atom types
    types->getTypedef(types->getBase(2, TYPE_UINT), "ATOM", 0, 0);
    
    // COLORREF (used in GDI)
    types->getTypedef(types->getBase(4, TYPE_UINT), "COLORREF", 0, 0);
    
    // File time
    types->getTypedef(types->getBase(8, TYPE_UINT), "FILETIME", 0, 0);
}

} // namespace types
} // namespace fission
