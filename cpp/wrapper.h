/**
 * Fission C Wrapper for Ghidra Decompiler
 * 
 * This header provides the C ABI interface for Rust FFI.
 * It wraps the C++ Ghidra decompiler with extern "C" functions.
 */

#ifndef FISSION_WRAPPER_H
#define FISSION_WRAPPER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Opaque handle to a Fission decompiler instance.
 * Internally wraps Ghidra's Architecture + Sleigh translator.
 */
typedef struct FissionDecompiler FissionDecompiler;

/**
 * Initialize a new decompiler instance.
 * 
 * @param sla_dir Path to directory containing .sla specification files
 * @return Pointer to decompiler instance, or NULL on failure
 */
FissionDecompiler* fission_decompiler_init(const char* sla_dir);

/**
 * Destroy a decompiler instance and free all resources.
 * 
 * @param decomp Pointer to decompiler instance
 */
void fission_decompiler_destroy(FissionDecompiler* decomp);

/**
 * Decompile a function at the given address.
 * 
 * @param decomp Decompiler instance
 * @param bytes Raw machine code bytes
 * @param bytes_len Length of bytes buffer
 * @param base_addr Virtual address of the function
 * @param out_buffer Buffer to write decompiled C code
 * @param out_len Maximum size of output buffer
 * @return Number of bytes written to out_buffer, or -1 on error
 */
int fission_decompile(
    FissionDecompiler* decomp,
    const uint8_t* bytes,
    size_t bytes_len,
    uint64_t base_addr,
    char* out_buffer,
    size_t out_len
);

/**
 * Disassemble instructions at the given address.
 * 
 * @param decomp Decompiler instance
 * @param bytes Raw machine code bytes
 * @param bytes_len Length of bytes buffer
 * @param base_addr Virtual address of the first instruction
 * @param out_buffer Buffer to write disassembly text
 * @param out_len Maximum size of output buffer
 * @return Number of bytes written to out_buffer, or -1 on error
 */
int fission_disassemble(
    FissionDecompiler* decomp,
    const uint8_t* bytes,
    size_t bytes_len,
    uint64_t base_addr,
    char* out_buffer,
    size_t out_len
);

/**
 * Get the last error message.
 * 
 * @return Pointer to static error string, or NULL if no error
 */
const char* fission_get_error(void);

/**
 * Check if native decompiler is available.
 * 
 * @return 1 if available, 0 otherwise
 */
int fission_is_available(void);

#ifdef __cplusplus
}
#endif

#endif // FISSION_WRAPPER_H
