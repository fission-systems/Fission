/**
 * Fission FID Manager
 * 
 * Manages FID (Function ID) database loading and matching.
 * Separated from libdecomp_ffi.cpp for better modularity.
 */

#ifndef FISSION_FFI_FID_MANAGER_H
#define FISSION_FFI_FID_MANAGER_H

#include "fission/ffi/DecompContext.h"
#include "fission/ffi/libdecomp_ffi.h"

namespace fission {
namespace ffi {

/**
 * Load a FID database
 * @param ctx Decompiler context
 * @param db_path Path to FID database file
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError load_fid_database(DecompContext* ctx, const char* db_path);

/**
 * Get FID match for a function
 * @param ctx Decompiler context
 * @param addr Function address
 * @param len Function length
 * @return Matched function name (caller must free) or nullptr
 */
char* get_fid_match(DecompContext* ctx, uint64_t addr, size_t len);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_FID_MANAGER_H
