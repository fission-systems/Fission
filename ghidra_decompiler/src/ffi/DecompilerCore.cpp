/**
 * Fission Decompiler Core Implementation (FFI wrapper)
 */

#include "fission/ffi/DecompilerCore.h"
#include "fission/decompiler/DecompilationCore.h"

#include <mutex>
#include <string>

void fission::ffi::ensure_architecture(DecompContext* ctx) {
    fission::decompiler::ensure_architecture(ctx);
}

std::string fission::ffi::run_decompilation(DecompContext* ctx, uint64_t addr) {
    return fission::decompiler::run_decompilation(ctx, addr);
}

std::string fission::ffi::run_decompilation_pcode(DecompContext* ctx, uint64_t addr) {
    return fission::decompiler::run_decompilation_pcode(ctx, addr);
}

void fission::ffi::set_gdt_path(DecompContext* ctx, const char* gdt_path) {
    if (!ctx) return;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->gdt_path = gdt_path ? gdt_path : "";
}

void fission::ffi::set_feature(DecompContext* ctx, const char* feature, bool enabled) {
    if (!ctx || !feature) return;

    std::lock_guard<std::mutex> lock(ctx->mutex);

    std::string feat(feature);

    if (feat == "infer_pointers") {
        ctx->infer_pointers = enabled;
    } else if (feat == "analyze_loops") {
        ctx->analyze_loops = enabled;
    } else if (feat == "readonly_propagate") {
        ctx->readonly_propagate = enabled;
    }
}
