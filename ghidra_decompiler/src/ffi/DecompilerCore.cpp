/**
 * Fission Decompiler Core Implementation (FFI wrapper)
 */

#include "fission/ffi/DecompilerCore.h"
#include "fission/decompiler/DecompilationCore.h"

#include <mutex>
#include <string>
#include "flow.hh"
#include "architecture.hh"
#include "options.hh"
#include "fission/utils/logger.h"

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

    if (feat == "infer_pointers" || feat == "inferconstptr") {
        ctx->infer_pointers = enabled;
    } else if (feat == "analyze_loops" || feat == "analyzeforloops") {
        ctx->analyze_loops = enabled;
    } else if (feat == "readonly_propagate" || feat == "readonly" || feat == "decompile.readonly") {
        ctx->readonly_propagate = enabled;
    } else if (feat == "record_jumploads" || feat == "jumpload") {
        ctx->record_jumploads = enabled;
    } else if (feat == "disable_toomanyinstructions_error") {
        ctx->disable_toomanyinstructions_error = enabled;
    } else if (feat == "errortoomanyinstructions") {
        // Original Ghidra option semantics: "on" means raise error.
        // Fission flag semantics are inverse (disable_*).
        ctx->disable_toomanyinstructions_error = !enabled;
    } else if (feat == "allow_inline" || feat == "inline") {
        ctx->allow_inline = enabled;
    // ── Post-processing options (pp_ prefix) ──
    } else if (feat == "pp_apply_struct_definitions") {
        ctx->post_process_options.apply_struct_definitions = enabled;
    } else if (feat == "pp_iat_symbols") {
        ctx->post_process_options.iat_symbols = enabled;
    } else if (feat == "pp_strip_shadow_params") {
        ctx->post_process_options.strip_shadow_params = enabled;
    } else if (feat == "pp_smart_constants") {
        ctx->post_process_options.smart_constants = enabled;
    } else if (feat == "pp_inline_strings") {
        ctx->post_process_options.inline_strings = enabled;
    } else if (feat == "pp_constants") {
        ctx->post_process_options.constants = enabled;
    } else if (feat == "pp_guids") {
        ctx->post_process_options.guids = enabled;
    } else if (feat == "pp_unicode_strings") {
        ctx->post_process_options.unicode_strings = enabled;
    } else if (feat == "pp_interlocked_patterns") {
        ctx->post_process_options.interlocked_patterns = enabled;
    } else if (feat == "pp_xunknown_types") {
        ctx->post_process_options.xunknown_types = enabled;
    } else if (feat == "pp_seh_cleanup") {
        ctx->post_process_options.seh_cleanup = enabled;
    } else if (feat == "pp_global_symbols") {
        ctx->post_process_options.global_symbols = enabled;
    } else if (feat == "pp_internal_names") {
        ctx->post_process_options.internal_names = enabled;
    } else if (feat == "pp_struct_offsets") {
        ctx->post_process_options.struct_offsets = enabled;
    } else if (feat == "pp_fid_names") {
        ctx->post_process_options.fid_names = enabled;
    } else {
        return;
    }

    // Keep runtime behavior consistent when toggled after architecture init.
    if (ctx->arch) {
        ctx->arch->infer_pointers = ctx->infer_pointers;
        ctx->arch->analyze_for_loops = ctx->analyze_loops;
        ctx->arch->readonlypropagate = ctx->readonly_propagate;

        if (ctx->record_jumploads) {
            ctx->arch->flowoptions |= ghidra::FlowInfo::record_jumploads;
        } else {
            ctx->arch->flowoptions &= ~ghidra::FlowInfo::record_jumploads;
        }

        if (ctx->disable_toomanyinstructions_error) {
            ctx->arch->flowoptions &= ~ghidra::FlowInfo::error_toomanyinstructions;
        } else {
            ctx->arch->flowoptions |= ghidra::FlowInfo::error_toomanyinstructions;
        }

        // Best-effort sync with original OptionDatabase toggles (from options.cc)
        if (ctx->arch->options != nullptr) {
            try {
                ctx->arch->options->set(ghidra::ELEM_INFERCONSTPTR.getId(), ctx->infer_pointers ? "on" : "off", "", "");
                ctx->arch->options->set(ghidra::ELEM_ANALYZEFORLOOPS.getId(), ctx->analyze_loops ? "on" : "off", "", "");
                ctx->arch->options->set(ghidra::ELEM_READONLY.getId(), ctx->readonly_propagate ? "on" : "off", "", "");
                ctx->arch->options->set(ghidra::ELEM_JUMPLOAD.getId(), ctx->record_jumploads ? "on" : "off", "", "");
                ctx->arch->options->set(ghidra::ELEM_ERRORTOOMANYINSTRUCTIONS.getId(), ctx->disable_toomanyinstructions_error ? "off" : "on", "", "");
                ctx->arch->options->set(ghidra::ELEM_INLINE.getId(), ctx->allow_inline ? "on" : "off", "", "");
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[DecompilerCore] set_feature: option sync failed: " << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[DecompilerCore] set_feature: option sync failed (unknown)" << std::endl;
            }
        }
    }
}
