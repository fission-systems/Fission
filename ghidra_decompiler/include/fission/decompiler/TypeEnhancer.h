#ifndef FISSION_DECOMPILER_TYPE_ENHANCER_H
#define FISSION_DECOMPILER_TYPE_ENHANCER_H

#include <string>
#include <map>
#include "funcdata.hh"
#include "type.hh"

namespace fission {
namespace decompiler {

/**
 * @brief Type enhancement and struct inference utilities
 * 
 * Provides functionality for:
 * - Reverse type propagation from call sites
 * - Struct type application to function parameters
 * - FID database file selection
 */
class TypeEnhancer {
public:
    /**
     * @brief Apply reverse type propagation
     * 
     * Scans CALL instructions in function. If target is in global registry,
     * updates argument types to match registered struct parameters.
     * 
     * @param fd Function data to analyze
     * @param type_factory Type factory for creating pointer types
     * @return true if any types were changed
     */
    static bool propagate_reverse_types(ghidra::Funcdata* fd, ghidra::TypeFactory* type_factory);
    
    /**
     * @brief Apply inferred struct types to C code output
     * 
     * Replaces generic pointer types (DWORD*, void*) with specific
     * struct pointer types in function parameter declarations.
     * 
     * @param c_code Original C code from decompiler
     * @param fd Function data with parameter information
     * @param structs Map of offset -> TypeStruct for inferred types
     * @return Modified C code with struct types applied
     */
    static std::string apply_struct_types(
        std::string c_code, 
        ghidra::Funcdata* fd,
        const std::map<unsigned long long, ghidra::TypeStruct*>& structs
    );
    
    /**
     * @brief Select appropriate FID database filename
     * 
     * Chooses FID database based on architecture and compiler ID.
     * 
     * @param is_64bit true for x64, false for x86
     * @param compiler_id Compiler identifier (vs2019, vs2017, vs2015, vs2012)
     * @return FID database filename (e.g., "vs2019_x64.fidbf")
     */
    static std::string get_fid_filename(bool is_64bit, const std::string& compiler_id);
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_TYPE_ENHANCER_H
