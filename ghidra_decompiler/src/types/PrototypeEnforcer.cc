#include "fission/types/PrototypeEnforcer.h"

// Ghidra includes
#include "architecture.hh"
#include "type.hh"
#include "fspec.hh"
#include "funcdata.hh"

#include <iostream>
#include <sstream>

namespace fission {
namespace types {

using namespace ghidra;

PrototypeEnforcer::PrototypeEnforcer() {}
PrototypeEnforcer::~PrototypeEnforcer() {}

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

bool PrototypeEnforcer::enforce_single_prototype(
    Architecture* arch,
    uint64_t address,
    const std::string& func_name
) {
    if (!arch || func_name.empty()) return false;

    TypeFactory* factory = arch->types;
    if (!factory) return false;

    // Try to find the function type by name in the TypeFactory
    Datatype* dt = factory->findByName(func_name);
    if (!dt) {
        // Try with common prefixes/suffixes stripped
        std::string alt_name = func_name;
        
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
        // Function type not found in GDT
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
