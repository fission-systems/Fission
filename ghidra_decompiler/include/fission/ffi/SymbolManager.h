/**
 * Fission Symbol Manager
 * 
 * Manages symbol table and function declarations.
 * Separated from libdecomp_ffi.cpp for better modularity.
 */

#ifndef FISSION_FFI_SYMBOL_MANAGER_H
#define FISSION_FFI_SYMBOL_MANAGER_H

#include "fission/ffi/DecompContext.h"
#include "fission/ffi/libdecomp_ffi.h"

namespace fission {
namespace ffi {

/**
 * Add a symbol to the context
 * @param ctx Decompiler context
 * @param addr Symbol address
 * @param name Symbol name
 */
void add_symbol(DecompContext* ctx, uint64_t addr, const char* name);

/**
 * Clear all symbols from the context
 * @param ctx Decompiler context
 */
void clear_symbols(DecompContext* ctx);

/**
 * Add a function declaration
 * @param ctx Decompiler context
 * @param addr Function address
 * @param name Function name (optional, will generate if null)
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError add_function(DecompContext* ctx, uint64_t addr, const char* name);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_SYMBOL_MANAGER_H
