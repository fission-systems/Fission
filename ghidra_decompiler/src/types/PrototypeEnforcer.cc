#include <cstring>
#include "fission/types/PrototypeEnforcer.h"

// Ghidra includes
#include "architecture.hh"
#include "type.hh"
#include "fspec.hh"
#include "funcdata.hh"

#include <fstream>
#include <iostream>
#include "fission/utils/logger.h"
#include <map>
#include <sstream>
#include <cctype>
#include <vector>

namespace fission {
namespace types {

using namespace ghidra;

PrototypeEnforcer::PrototypeEnforcer() {}
PrototypeEnforcer::~PrototypeEnforcer() {}

static std::string canonicalize_name(const std::string& name) {
    std::string result = name;
    size_t bang_pos = result.rfind('!');
    if (bang_pos != std::string::npos && bang_pos + 1 < result.size()) {
        result = result.substr(bang_pos + 1);
    }
    if (result.rfind("__imp__", 0) == 0) {
        result = result.substr(7);
    } else if (result.rfind("__imp_", 0) == 0) {
        result = result.substr(6);
    }

    while (!result.empty() && result[0] == '_') {
        result.erase(result.begin());
    }

    size_t at_pos = result.find('@');
    if (at_pos != std::string::npos) {
        result = result.substr(0, at_pos);
    }

    return result;
}

static std::string to_lower_copy(const std::string& name) {
    std::string lower = name;
    for (char& ch : lower) {
        ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
    }
    return lower;
}

struct WinApiParamDef {
    std::string name;
    std::string type;
};

struct WinApiSignatureDef {
    std::string name;
    std::string return_type;
    std::vector<WinApiParamDef> params;
};

static std::map<std::string, WinApiSignatureDef> win_api_db;
static bool win_api_loaded = false;

static std::string trim_copy(const std::string& s) {
    size_t start = 0;
    while (start < s.size() && std::isspace(static_cast<unsigned char>(s[start]))) {
        start++;
    }
    size_t end = s.size();
    while (end > start && std::isspace(static_cast<unsigned char>(s[end - 1]))) {
        end--;
    }
    return s.substr(start, end - start);
}

static void load_win_api_db() {
    if (win_api_loaded) {
        return;
    }
    win_api_loaded = true;

    const std::vector<std::string> paths = {
        "utils/signatures/typeinfo/win32/win_api_signatures.txt",
        "./utils/signatures/typeinfo/win32/win_api_signatures.txt",
        "../utils/signatures/typeinfo/win32/win_api_signatures.txt",
        "ghidra_decompiler/typeinfo/win32/win_api_signatures.txt",
        "./ghidra_decompiler/typeinfo/win32/win_api_signatures.txt"
    };

    std::ifstream file;
    for (const auto& path : paths) {
        file.open(path);
        if (file.is_open()) {
            break;
        }
    }

    if (!file.is_open()) {
        return;
    }

    std::string line;
    while (std::getline(file, line)) {
        if (line.empty() || line[0] == '#') {
            continue;
        }

        size_t first = line.find('|');
        if (first == std::string::npos) {
            continue;
        }
        size_t second = line.find('|', first + 1);
        if (second == std::string::npos) {
            continue;
        }

        WinApiSignatureDef sig;
        sig.name = trim_copy(line.substr(0, first));
        sig.return_type = trim_copy(line.substr(first + 1, second - first - 1));
        std::string params_blob = trim_copy(line.substr(second + 1));

        if (!params_blob.empty()) {
            size_t start = 0;
            while (start < params_blob.size()) {
                size_t end = params_blob.find(',', start);
                std::string param = params_blob.substr(
                    start,
                    end == std::string::npos ? std::string::npos : end - start
                );
                size_t colon = param.find(':');
                if (colon != std::string::npos) {
                    WinApiParamDef p;
                    p.name = trim_copy(param.substr(0, colon));
                    p.type = trim_copy(param.substr(colon + 1));
                    if (!p.name.empty() && !p.type.empty()) {
                        sig.params.push_back(p);
                    }
                }
                if (end == std::string::npos) {
                    break;
                }
                start = end + 1;
            }
        }

        if (!sig.name.empty()) {
            std::string key = to_lower_copy(canonicalize_name(sig.name));
            win_api_db[key] = sig;
        }
    }
}

static Datatype* resolve_winapi_type(TypeFactory* factory, const std::string& type_name) {
    if (!factory) {
        return nullptr;
    }

    std::string raw = trim_copy(type_name);
    if (raw.empty()) {
        return nullptr;
    }

    if (Datatype* dt = factory->findByName(raw)) {
        return dt;
    }

    std::string stripped = raw;
    auto strip_prefix_no_case = [&](const std::string& prefix) {
        std::string lower = to_lower_copy(stripped);
        if (lower.rfind(prefix, 0) == 0) {
            stripped = trim_copy(stripped.substr(prefix.size()));
            return true;
        }
        return false;
    };

    bool changed = true;
    while (changed) {
        changed = false;
        changed |= strip_prefix_no_case("const ");
        changed |= strip_prefix_no_case("volatile ");
        changed |= strip_prefix_no_case("struct ");
        changed |= strip_prefix_no_case("enum ");
    }

    std::string nospace;
    nospace.reserve(stripped.size());
    for (char ch : stripped) {
        if (!std::isspace(static_cast<unsigned char>(ch))) {
            nospace.push_back(ch);
        }
    }

    int pointer_depth = 0;
    while (!nospace.empty() && nospace.back() == '*') {
        pointer_depth++;
        nospace.pop_back();
    }

    std::string base = nospace;
    std::string lower = to_lower_copy(base);
    int ptr_size = factory->getSizeOfPointer();
    Datatype* void_type = factory->getTypeVoid();
    Datatype* void_ptr = void_type ? factory->getTypePointer(ptr_size, void_type, 0) : nullptr;
    Datatype* char_type = factory->getTypeChar(factory->getSizeOfChar());
    Datatype* char_ptr = char_type ? factory->getTypePointer(ptr_size, char_type, 0) : nullptr;
    Datatype* wchar_type = factory->getBase(2, TYPE_UINT);
    Datatype* wchar_ptr = wchar_type ? factory->getTypePointer(ptr_size, wchar_type, 0) : nullptr;

    if (base == "BOOL" || lower == "bool") return factory->getBase(4, TYPE_INT);
    if (base == "BOOLEAN" || lower == "boolean") return factory->getBase(1, TYPE_UINT);
    if (lower == "void") return void_type;
    if (lower == "char") return char_type;
    if (lower == "wchar_t" || lower == "wchar") return wchar_type;
    if (lower == "byte" || lower == "uchar") return factory->getBase(1, TYPE_UINT);
    if (lower == "word" || lower == "ushort") return factory->getBase(2, TYPE_UINT);
    if (lower == "short") return factory->getBase(2, TYPE_INT);
    if (lower == "int") return factory->getBase(factory->getSizeOfInt(), TYPE_INT);
    if (lower == "uint") return factory->getBase(factory->getSizeOfInt(), TYPE_UINT);
    if (lower == "dword" || lower == "ulong" || lower == "uint32" || lower == "uint32_t") {
        return factory->getBase(4, TYPE_UINT);
    }
    if (lower == "long" || lower == "int32" || lower == "int32_t") {
        return factory->getBase(4, TYPE_INT);
    }
    if (lower == "longlong" || lower == "int64" || lower == "int64_t") {
        return factory->getBase(8, TYPE_INT);
    }
    if (lower == "ulonglong" || lower == "uint64" || lower == "uint64_t" || lower == "dword64") {
        return factory->getBase(8, TYPE_UINT);
    }
    if (lower == "ntstatus" || lower == "hresult") return factory->getBase(4, TYPE_INT);
    if (lower == "lresult" || lower == "long_ptr" || lower == "ssize_t") {
        return factory->getBase(ptr_size, TYPE_INT);
    }
    if (lower == "wparam" || lower == "lparam" || lower == "uintptr_t" ||
        lower == "uint_ptr" || lower == "ulong_ptr" || lower == "dword_ptr" ||
        lower == "size_t" || lower == "uintptr" || lower == "size") {
        return factory->getBase(ptr_size, TYPE_UINT);
    }

    if (lower == "lpstr" || lower == "lpcstr" || lower == "pstr" ||
        lower == "pcstr" || lower == "lpsz" || lower == "psz" ||
        lower == "tchar" || lower == "lptstr" || lower == "lpctstr") {
        return char_ptr;
    }
    if (lower == "lpwstr" || lower == "lpcwstr" || lower == "pwstr" ||
        lower == "pcwstr" || lower == "lpszw") {
        return wchar_ptr;
    }
    if (lower == "lpvoid" || lower == "pvoid" || lower == "lpctvoid" ||
        lower == "lpcvoid" || lower == "voidptr") {
        return void_ptr;
    }

    if (lower == "handle" || lower == "hmodule" || lower == "hinstance" ||
        lower == "hwnd" || lower == "hkey" || lower == "hprocess" || lower == "hthread") {
        return void_ptr;
    }

    if (lower.find("wstr") != std::string::npos || lower.find("wchar") != std::string::npos) {
        return wchar_ptr;
    }
    if (lower.find("str") != std::string::npos) {
        return char_ptr;
    }

    if (Datatype* dt = factory->findByName(base)) {
        if (pointer_depth == 0) {
            return dt;
        }
        Datatype* current = dt;
        for (int i = 0; i < pointer_depth; i++) {
            current = factory->getTypePointer(ptr_size, current, 0);
        }
        return current;
    }

    if (pointer_depth == 0) {
        if (lower.rfind("lp", 0) == 0 || lower.rfind("p", 0) == 0) {
            std::string prefix = (lower.rfind("lp", 0) == 0) ? lower.substr(2) : lower.substr(1);
            if (!prefix.empty()) {
                Datatype* base_dt = resolve_winapi_type(factory, prefix);
                if (base_dt) {
                    return factory->getTypePointer(ptr_size, base_dt, 0);
                }
            }
        }
    }

    if (pointer_depth > 0) {
        Datatype* base_dt = resolve_winapi_type(factory, base);
        if (base_dt) {
            Datatype* current = base_dt;
            for (int i = 0; i < pointer_depth; i++) {
                current = factory->getTypePointer(ptr_size, current, 0);
            }
            return current;
        }
        return void_ptr;
    }

    return nullptr;
}

static bool build_win_api_prototype(
    Architecture* arch,
    const std::string& func_name,
    PrototypePieces& out_pieces
) {
    if (!arch) {
        return false;
    }

    load_win_api_db();
    if (win_api_db.empty()) {
        return false;
    }

    std::string key = to_lower_copy(canonicalize_name(func_name));
    auto it = win_api_db.find(key);
    if (it == win_api_db.end()) {
        return false;
    }

    TypeFactory* factory = arch->types;
    if (!factory) {
        return false;
    }

    Datatype* return_type = resolve_winapi_type(factory, it->second.return_type);
    if (!return_type) {
        return false;
    }

    std::vector<Datatype*> param_types;
    std::vector<std::string> param_names;

    for (const auto& param : it->second.params) {
        Datatype* dt = resolve_winapi_type(factory, param.type);
        if (!dt) {
            dt = factory->getBase(factory->getSizeOfInt(), TYPE_INT);
        }
        param_types.push_back(dt);
        param_names.push_back(param.name);
    }

    ProtoModel* model = nullptr;
    int ptr_size = factory->getSizeOfPointer();
    if (ptr_size >= 8) {
        model = arch->getModel("__fastcall");
    } else {
        model = arch->getModel("__stdcall");
    }
    if (!model) {
        model = arch->getModel("__cdecl");
    }
    if (!model) {
        return false;
    }

    out_pieces.model = model;
    out_pieces.name = func_name;
    out_pieces.outtype = return_type;
    out_pieces.intypes = param_types;
    out_pieces.innames = param_names;
    out_pieces.firstVarArgSlot = -1;
    return true;
}

static bool build_varargs_prototype(
    Architecture* arch,
    const std::string& func_name,
    const std::string& canonical,
    PrototypePieces& out_pieces
) {
    if (!arch || canonical.empty()) {
        return false;
    }

    TypeFactory* factory = arch->types;
    if (!factory) {
        return false;
    }

    int4 ptr_size = factory->getSizeOfPointer();
    Datatype* int_type = factory->getBase(factory->getSizeOfInt(), TYPE_INT);
    Datatype* char_type = factory->getTypeChar(factory->getSizeOfChar());
    Datatype* wchar_type = factory->getBase(2, TYPE_UINT);
    Datatype* void_type = factory->getTypeVoid();
    if (!int_type || !char_type || !wchar_type || !void_type || ptr_size <= 0) {
        return false;
    }

    Datatype* char_ptr = factory->getTypePointer(ptr_size, char_type, 0);
    Datatype* wchar_ptr = factory->getTypePointer(ptr_size, wchar_type, 0);
    Datatype* void_ptr = factory->getTypePointer(ptr_size, void_type, 0);
    Datatype* size_type = factory->getBase(ptr_size, TYPE_UINT);
    if (!char_ptr || !wchar_ptr || !void_ptr || !size_type) {
        return false;
    }

    ProtoModel* model = arch->getModel("__cdecl");
    if (!model) {
        model = arch->getModel("__fastcall");
    }

    auto set_common = [&](const std::vector<Datatype*>& types,
                          const std::vector<std::string>& names,
                          int4 first_vararg) {
        out_pieces.model = model;
        out_pieces.name = func_name;
        out_pieces.outtype = int_type;
        out_pieces.intypes = types;
        out_pieces.innames = names;
        out_pieces.firstVarArgSlot = first_vararg;
    };

    bool wide = false;
    std::string name = canonical;
    // Common MinGW/GNU wrappers should use the same vararg model.
    if (name.rfind("mingw_", 0) == 0) {
        name = name.substr(6);
    } else if (name.rfind("gnu_", 0) == 0) {
        name = name.substr(4);
    }

    if (name == "printf" || name == "scanf" || name == "wprintf" || name == "wscanf") {
        wide = (name[0] == 'w');
        Datatype* format_ptr = wide ? wchar_ptr : char_ptr;
        set_common({format_ptr}, {"format"}, 1);
        return true;
    }

    if (name == "fprintf" || name == "fscanf" || name == "fwprintf" || name == "fwscanf") {
        wide = (name[0] == 'f' && name[1] == 'w');
        Datatype* format_ptr = wide ? wchar_ptr : char_ptr;
        set_common({void_ptr, format_ptr}, {"stream", "format"}, 2);
        return true;
    }

    if (name == "sprintf" || name == "sscanf" || name == "swprintf" || name == "swscanf") {
        wide = (name[0] == 's' && name[1] == 'w');
        Datatype* text_ptr = wide ? wchar_ptr : char_ptr;
        Datatype* format_ptr = wide ? wchar_ptr : char_ptr;
        set_common({text_ptr, format_ptr}, {"buffer", "format"}, 2);
        return true;
    }

    if (name == "snprintf" || name == "snwprintf") {
        wide = (name == "snwprintf");
        Datatype* text_ptr = wide ? wchar_ptr : char_ptr;
        Datatype* format_ptr = wide ? wchar_ptr : char_ptr;
        set_common({text_ptr, size_type, format_ptr}, {"buffer", "size", "format"}, 3);
        return true;
    }

    return false;
}

static Datatype* resolve_or_create_struct_ptr(TypeFactory* factory, const std::string& struct_name) {
    if (!factory || struct_name.empty()) {
        return nullptr;
    }
    const int ptr_size = factory->getSizeOfPointer();
    Datatype* dt = factory->findByName(struct_name);
    TypeStruct* st = nullptr;
    if (dt && dt->getMetatype() == TYPE_STRUCT) {
        st = static_cast<TypeStruct*>(dt);
    } else {
        st = factory->getTypeStruct(struct_name);
    }
    if (!st) {
        return nullptr;
    }
    return factory->getTypePointer(ptr_size, st, 0);
}

bool PrototypeEnforcer::build_prototype_pieces(
    Architecture* arch,
    const std::string& func_name,
    TypeCode* func_type,
    PrototypePieces& out_pieces
) {
    if (!func_type) return false;
    
    // Get the FuncProto from the TypeCode
    const FuncProto* proto = func_type->getPrototype();
    if (!proto) return false;

    // Use getPieces() to fill the PrototypePieces directly
    proto->getPieces(out_pieces);
    
    // Override the name with the actual function name
    out_pieces.name = func_name;

    return true;
}

bool PrototypeEnforcer::build_builtin_prototype(
    Architecture* arch,
    const std::string& func_name,
    PrototypePieces& out_pieces
) {
    if (!arch || func_name.empty()) {
        return false;
    }

    TypeFactory* factory = arch->types;
    if (!factory) {
        return false;
    }

    std::string canonical = to_lower_copy(canonicalize_name(func_name));
    std::string lower_name = to_lower_copy(func_name);
    if (lower_name == "__main") {
        ProtoModel* model = arch->getModel("__cdecl");
        if (!model) {
            model = arch->getModel("__fastcall");
        }
        out_pieces.model = model;
        out_pieces.name = func_name;
        out_pieces.outtype = factory->getTypeVoid();
        out_pieces.intypes.clear();
        out_pieces.innames.clear();
        out_pieces.firstVarArgSlot = -1;
        return true;
    }

    if (build_varargs_prototype(arch, func_name, canonical, out_pieces)) {
        return true;
    }

    int4 ptr_size = factory->getSizeOfPointer();
    Datatype* int_type = factory->getBase(factory->getSizeOfInt(), TYPE_INT);
    Datatype* char_type = factory->getTypeChar(factory->getSizeOfChar());
    Datatype* void_type = factory->getTypeVoid();
    Datatype* wchar_type = factory->getBase(2, TYPE_UINT);
    if (!int_type || !char_type || !void_type || ptr_size <= 0) {
        return false;
    }

    Datatype* char_ptr = factory->getTypePointer(ptr_size, char_type, 0);
    Datatype* char_ptr_ptr = factory->getTypePointer(ptr_size, char_ptr, 0);
    Datatype* void_ptr = factory->getTypePointer(ptr_size, void_type, 0);
    Datatype* size_type = factory->getBase(ptr_size, TYPE_UINT);
    Datatype* wchar_ptr = wchar_type ? factory->getTypePointer(ptr_size, wchar_type, 0) : nullptr;
    if (!char_ptr || !char_ptr_ptr || !void_ptr || !size_type) {
        return false;
    }

    auto set_simple = [&](Datatype* out,
                          const std::vector<Datatype*>& types,
                          const std::vector<std::string>& names) {
        ProtoModel* model = nullptr;
        if (ptr_size >= 8) {
            model = arch->getModel("__fastcall");
            if (!model) {
                model = arch->getModel("windows");
            }
        }
        if (!model) {
            model = arch->getModel("__cdecl");
        }
        if (!model) {
            model = arch->getModel("default");
        }
        out_pieces.model = model;
        out_pieces.name = func_name;
        out_pieces.outtype = out;
        out_pieces.intypes = types;
        out_pieces.innames = names;
        out_pieces.firstVarArgSlot = -1;
    };

    if (canonical == "puts") {
        set_simple(int_type, {char_ptr}, {"str"});
        return true;
    }
    if (canonical == "strlen") {
        set_simple(size_type, {char_ptr}, {"str"});
        return true;
    }
    if (canonical == "malloc") {
        set_simple(void_ptr, {size_type}, {"size"});
        return true;
    }
    if (canonical == "calloc") {
        set_simple(void_ptr, {size_type, size_type}, {"nmemb", "size"});
        return true;
    }
    if (canonical == "free") {
        set_simple(void_type, {void_ptr}, {"ptr"});
        return true;
    }
    if (canonical == "memcpy") {
        set_simple(void_ptr, {void_ptr, void_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }
    if (canonical == "memmove") {
        set_simple(void_ptr, {void_ptr, void_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }
    if (canonical == "memcmp") {
        set_simple(int_type, {void_ptr, void_ptr, size_type}, {"lhs", "rhs", "count"});
        return true;
    }
    if (canonical == "memset") {
        set_simple(void_ptr, {void_ptr, int_type, size_type}, {"dest", "value", "count"});
        return true;
    }
    if (canonical == "strcpy") {
        set_simple(char_ptr, {char_ptr, char_ptr}, {"dest", "src"});
        return true;
    }
    if (canonical == "strncpy") {
        set_simple(char_ptr, {char_ptr, char_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }
    if (canonical == "strcat") {
        set_simple(char_ptr, {char_ptr, char_ptr}, {"dest", "src"});
        return true;
    }
    if (canonical == "strncat") {
        set_simple(char_ptr, {char_ptr, char_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }
    if (canonical == "strcmp") {
        set_simple(int_type, {char_ptr, char_ptr}, {"lhs", "rhs"});
        return true;
    }
    if (canonical == "strncmp") {
        set_simple(int_type, {char_ptr, char_ptr, size_type}, {"lhs", "rhs", "count"});
        return true;
    }
    if (canonical == "strnlen") {
        set_simple(size_type, {char_ptr, size_type}, {"str", "maxlen"});
        return true;
    }
    if (canonical == "strchr") {
        set_simple(char_ptr, {char_ptr, int_type}, {"str", "ch"});
        return true;
    }
    if (canonical == "strrchr") {
        set_simple(char_ptr, {char_ptr, int_type}, {"str", "ch"});
        return true;
    }
    if (canonical == "wcslen" && wchar_ptr) {
        set_simple(size_type, {wchar_ptr}, {"str"});
        return true;
    }
    if (canonical == "wcscmp" && wchar_ptr) {
        set_simple(int_type, {wchar_ptr, wchar_ptr}, {"lhs", "rhs"});
        return true;
    }
    if (canonical == "wcsncmp" && wchar_ptr) {
        set_simple(int_type, {wchar_ptr, wchar_ptr, size_type}, {"lhs", "rhs", "count"});
        return true;
    }
    if (canonical == "wcscpy" && wchar_ptr) {
        set_simple(wchar_ptr, {wchar_ptr, wchar_ptr}, {"dest", "src"});
        return true;
    }
    if (canonical == "wcsncpy" && wchar_ptr) {
        set_simple(wchar_ptr, {wchar_ptr, wchar_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }
    if (canonical == "wcscat" && wchar_ptr) {
        set_simple(wchar_ptr, {wchar_ptr, wchar_ptr}, {"dest", "src"});
        return true;
    }
    if (canonical == "wcsncat" && wchar_ptr) {
        set_simple(wchar_ptr, {wchar_ptr, wchar_ptr, size_type}, {"dest", "src", "count"});
        return true;
    }

    // Internal user-defined signatures (seed set).
    // These are applied when function names are known (e.g. from symbol provider/export table).
    if (canonical == "cpp_add") {
        set_simple(int_type, {int_type, int_type}, {"a", "b"});
        return true;
    }
    if (canonical == "cpp_switch") {
        set_simple(int_type, {int_type}, {"x"});
        return true;
    }
    if (canonical == "cpp_sum_array") {
        Datatype* int_ptr = factory->getTypePointer(ptr_size, int_type, 0);
        if (int_ptr) {
            set_simple(int_type, {int_ptr, int_type}, {"arr", "len"});
            return true;
        }
    }
    if (canonical == "cpp_init_item") {
        Datatype* item_ptr = resolve_or_create_struct_ptr(factory, "Item");
        if (!item_ptr) item_ptr = void_ptr;
        Datatype* double_type = factory->getBase(8, TYPE_FLOAT);
        if (item_ptr && double_type) {
            set_simple(void_type, {item_ptr, int_type, char_ptr, double_type}, {"item", "id", "name", "value"});
            return true;
        }
    }
    if (canonical == "cpp_create_item") {
        Datatype* item_ptr = resolve_or_create_struct_ptr(factory, "Item");
        if (!item_ptr) item_ptr = void_ptr;
        Datatype* double_type = factory->getBase(8, TYPE_FLOAT);
        if (item_ptr && double_type) {
            set_simple(item_ptr, {int_type, char_ptr, double_type}, {"id", "name", "value"});
            return true;
        }
    }
    if (canonical == "cpp_destroy_item") {
        Datatype* item_ptr = resolve_or_create_struct_ptr(factory, "Item");
        if (!item_ptr) item_ptr = void_ptr;
        if (item_ptr) {
            set_simple(void_type, {item_ptr}, {"item"});
            return true;
        }
    }
    if (canonical == "cpp_virtual_compute") {
        set_simple(int_type, {int_type}, {"x"});
        return true;
    }
    if (canonical == "cpp_main_like") {
        set_simple(int_type, {}, {});
        return true;
    }

    if (canonical != "main" && canonical != "wmain") {
        return false;
    }
    if (func_name.rfind("__", 0) == 0 &&
        func_name.rfind("__imp_", 0) != 0 &&
        func_name.rfind("__imp__", 0) != 0) {
        return false;
    }

    ProtoModel* model = arch->getModel("__cdecl");
    if (!model) {
        model = arch->getModel("__fastcall");
    }

    out_pieces.model = model;
    out_pieces.name = func_name;
    out_pieces.outtype = int_type;
    out_pieces.intypes = { int_type, char_ptr_ptr, char_ptr_ptr };
    out_pieces.innames = { "_Argc", "_Argv", "_Env" };
    out_pieces.firstVarArgSlot = -1;
    return true;
}

bool PrototypeEnforcer::enforce_single_prototype(
    Architecture* arch,
    uint64_t address,
    const std::string& func_name
) {
    if (!arch || func_name.empty()) return false;

    TypeFactory* factory = arch->types;
    if (!factory) return false;

    // Try to find the function type by name in the TypeFactory
    std::string lookup_name = canonicalize_name(func_name);
    Datatype* dt = factory->findByName(lookup_name);
    if (!dt) {
        // Try with common prefixes/suffixes stripped
        std::string alt_name = lookup_name;
        
        // Remove leading underscore
        if (!alt_name.empty() && alt_name[0] == '_') {
            alt_name = alt_name.substr(1);
            dt = factory->findByName(alt_name);
        }
        
        // Try without 'W' or 'A' suffix (Windows ANSI/Unicode variants)
        if (!dt && alt_name.length() > 1) {
            char last = alt_name[alt_name.length() - 1];
            if (last == 'W' || last == 'A') {
                std::string base_name = alt_name.substr(0, alt_name.length() - 1);
                dt = factory->findByName(base_name);
            }
        }
    }

    if (!dt) {
        // Fall back to built-in signatures for well-known entry points.
        PrototypePieces pieces;
        if (build_builtin_prototype(arch, func_name, pieces)) {
            try {
                arch->setPrototype(pieces);
                fission::utils::log_stream() << "[PrototypeEnforcer] Applied built-in prototype for: "
                          << func_name << std::endl;
                return true;
            } catch (const LowlevelError& e) {
                fission::utils::log_stream() << "[PrototypeEnforcer] Error applying built-in prototype for "
                          << func_name << ": " << e.explain << std::endl;
            }
        }
        if (build_win_api_prototype(arch, func_name, pieces)) {
            try {
                arch->setPrototype(pieces);
                fission::utils::log_stream() << "[PrototypeEnforcer] Applied WinAPI prototype for: "
                          << func_name << std::endl;
                return true;
            } catch (const LowlevelError& e) {
                fission::utils::log_stream() << "[PrototypeEnforcer] Error applying WinAPI prototype for "
                          << func_name << ": " << e.explain << std::endl;
            }
        }
        return false;
    }

    // Check if it's a function type (TypeCode)
    if (dt->getMetatype() != TYPE_CODE) {
        return false;
    }

    TypeCode* func_type = (TypeCode*)dt;
    
    // Build PrototypePieces from the TypeCode
    PrototypePieces pieces;
    if (!build_prototype_pieces(arch, func_name, func_type, pieces)) {
        return false;
    }

    // Apply the prototype to the architecture at this address
    try {
        arch->setPrototype(pieces);
        fission::utils::log_stream() << "[PrototypeEnforcer] Applied prototype for: " << func_name 
                  << " (" << pieces.intypes.size() << " params)" << std::endl;
        return true;
    } catch (const LowlevelError& e) {
        fission::utils::log_stream() << "[PrototypeEnforcer] Error applying prototype for " << func_name 
                  << ": " << e.explain << std::endl;
        return false;
    }
}

int PrototypeEnforcer::enforce_iat_prototypes(
    Architecture* arch,
    const std::map<uint64_t, std::string>& iat_symbols
) {
    int count = 0;

    for (const auto& pair : iat_symbols) {
        uint64_t address = pair.first;
        const std::string& name = pair.second;

        if (enforce_single_prototype(arch, address, name)) {
            ++count;
        }
    }

    if (count > 0) {
        fission::utils::log_stream() << "[PrototypeEnforcer] Enforced " << count << "/" << iat_symbols.size() 
                  << " IAT prototypes" << std::endl;
    }

    return count;
}

} // namespace types
} // namespace fission
