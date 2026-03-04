#ifndef __PROTOTYPE_ENFORCER_H__
#define __PROTOTYPE_ENFORCER_H__

#include <cstdint>
#include <map>
#include <string>
#include <vector>

// Forward declarations for Ghidra types
namespace ghidra {
    class Architecture;
    class TypeFactory;
    class Datatype;
    class TypeCode;
    class FuncProto;
    struct PrototypePieces;
}

namespace fission {
namespace types {

/// \brief Enforces function prototypes from GDT onto IAT symbols
///
/// This class looks up function signatures in the TypeFactory (loaded from GDT)
/// and applies them to IAT function addresses before decompilation.
class PrototypeEnforcer {
private:
    /// Build PrototypePieces from a TypeCode (function type)
    bool build_prototype_pieces(
        ghidra::Architecture* arch,
        const std::string& func_name,
        ghidra::TypeCode* func_type,
        ghidra::PrototypePieces& out_pieces
    );
    bool build_builtin_prototype(
        ghidra::Architecture* arch,
        const std::string& func_name,
        ghidra::PrototypePieces& out_pieces
    );

public:
    PrototypeEnforcer();
    ~PrototypeEnforcer();

    /// \brief Enforce prototypes for all IAT symbols
    /// \param arch The Architecture context
    /// \param iat_symbols Map of address -> symbol name
    /// \return Number of prototypes successfully applied
    int enforce_iat_prototypes(
        ghidra::Architecture* arch,
        const std::map<uint64_t, std::string>& iat_symbols
    );

    /// \brief Enforce prototype for a single function
    /// \param arch The Architecture context
    /// \param address Function address
    /// \param func_name Function name to lookup in GDT
    /// \return true if prototype was applied
    bool enforce_single_prototype(
        ghidra::Architecture* arch,
        uint64_t address,
        const std::string& func_name
    );
};

} // namespace types
} // namespace fission

#endif // __PROTOTYPE_ENFORCER_H__
