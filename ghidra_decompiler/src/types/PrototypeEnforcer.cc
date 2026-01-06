#include "fission/types/PrototypeEnforcer.h"

// Ghidra includes
#include "architecture.hh"
#include "type.hh"
#include "fspec.hh"
#include "funcdata.hh"

#include <iostream>
#include <sstream>
#include <cctype>

namespace fission {
namespace types {

using namespace ghidra;

PrototypeEnforcer::PrototypeEnforcer() {}
PrototypeEnforcer::~PrototypeEnforcer() {}

static std::string canonicalize_name(const std::string& name) {
    std::string result = name;
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
    if (canonical != "main" && canonical != "wmain") {
        return false;
    }
    if (func_name.rfind("__", 0) == 0 &&
        func_name.rfind("__imp_", 0) != 0 &&
        func_name.rfind("__imp__", 0) != 0) {
        return false;
    }

    int4 ptr_size = factory->getSizeOfPointer();
    Datatype* int_type = factory->getBase(factory->getSizeOfInt(), TYPE_INT);
    Datatype* char_type = factory->getTypeChar(factory->getSizeOfChar());
    if (!int_type || !char_type || ptr_size <= 0) {
        return false;
    }

    Datatype* char_ptr = factory->getTypePointer(ptr_size, char_type, 0);
    Datatype* char_ptr_ptr = factory->getTypePointer(ptr_size, char_ptr, 0);
    if (!char_ptr || !char_ptr_ptr) {
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
                std::cerr << "[PrototypeEnforcer] Applied built-in prototype for: "
                          << func_name << std::endl;
                return true;
            } catch (const LowlevelError& e) {
                std::cerr << "[PrototypeEnforcer] Error applying built-in prototype for "
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
        std::cerr << "[PrototypeEnforcer] Applied prototype for: " << func_name 
                  << " (" << pieces.intypes.size() << " params)" << std::endl;
        return true;
    } catch (const LowlevelError& e) {
        std::cerr << "[PrototypeEnforcer] Error applying prototype for " << func_name 
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
        std::cerr << "[PrototypeEnforcer] Enforced " << count << "/" << iat_symbols.size() 
                  << " IAT prototypes" << std::endl;
    }

    return count;
}

} // namespace types
} // namespace fission
