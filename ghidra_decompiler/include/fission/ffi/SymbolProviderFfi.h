#ifndef FISSION_FFI_SYMBOL_PROVIDER_FFI_H
#define FISSION_FFI_SYMBOL_PROVIDER_FFI_H

#include <cstdint>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct DecompSymbolInfo {
    uint64_t address;
    uint32_t size;
    uint32_t flags;
    const char* name;
    uint32_t name_len;
} DecompSymbolInfo;

typedef int (*DecompFindSymbolFn)(
    void* userdata,
    uint64_t address,
    uint32_t size,
    int require_start,
    DecompSymbolInfo* out
);

typedef int (*DecompFindFunctionFn)(
    void* userdata,
    uint64_t address,
    DecompSymbolInfo* out
);

typedef void (*DecompProviderDropFn)(void* userdata);

typedef struct DecompSymbolProvider {
    void* userdata;
    DecompFindSymbolFn find_symbol;
    DecompFindFunctionFn find_function;
    DecompProviderDropFn drop;
} DecompSymbolProvider;

#ifdef __cplusplus
}
#endif

#endif // FISSION_FFI_SYMBOL_PROVIDER_FFI_H
