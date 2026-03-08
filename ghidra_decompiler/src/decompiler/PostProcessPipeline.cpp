/**
 * Fission Decompiler Post-Processing Pipeline
 */

#include "fission/decompiler/PostProcessPipeline.h"
#include "fission/config/PathConfig.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/StringScanner.h"
#include "fission/types/GuidParser.h"
#include "fission/utils/file_utils.h"
#include "fission/ffi/DecompContext.h"
#include "fission/decompiler/PostProcessor.h"

#include "address.hh"

#include <map>
#include <regex>
#include <set>
#include <string>
#include <chrono>
#include <unordered_set>
#include <vector>

using namespace fission::processing;
using namespace fission::analysis;
using namespace fission::utils;

namespace fission {
namespace decompiler {

static std::map<std::string, std::string> load_guid_maps() {
    std::map<std::string, std::string> guid_map;

    std::vector<std::string> guid_files = fission::config::get_guid_files();

    for (const auto& path : guid_files) {
        if (file_exists(path)) {
            std::string content = read_file_content(path);
            if (!content.empty()) {
                std::map<std::string, std::string> loaded =
                    fission::types::load_guids_to_map(content);
                guid_map.insert(loaded.begin(), loaded.end());
            }
        }
    }

    return guid_map;
}

static const std::map<std::string, std::string>& get_guid_map() {
    static std::map<std::string, std::string> guid_map = load_guid_maps();
    return guid_map;
}

namespace {

size_t count_occurrences_limited(
    const std::string& text,
    const std::string& needle,
    size_t limit
) {
    if (text.empty() || needle.empty() || limit == 0) {
        return 0;
    }

    size_t count = 0;
    size_t pos = 0;
    while ((pos = text.find(needle, pos)) != std::string::npos) {
        ++count;
        if (count >= limit) {
            break;
        }
        pos += needle.size();
    }
    return count;
}

struct PostProcessHotPathPolicy {
    bool is_large_function = false;
    bool is_huge_function = false;
    bool is_giant_dispatcher = false;
    bool is_extreme_dispatcher = false;
    bool has_stack_names = false;
    bool use_fast_postprocessor = false;
    bool skip_postprocessor = false;
    bool skip_smart_constant_replace = false;
    bool skip_constant_cleanup = false;
    bool skip_demangle_cleanup = false;
    bool skip_virtual_call_cleanup = false;
    bool skip_standardize_variable_names = false;
    bool skip_signature_cleanup = false;
    bool skip_internal_name_cleanup = false;
    bool skip_struct_offset_annotation = false;
    bool skip_fid_resolution = false;
};

PostProcessHotPathPolicy build_postprocess_policy(
    const std::string& code,
    const AnalysisArtifacts& analysis
) {
    PostProcessHotPathPolicy policy;
    policy.is_large_function = code.size() > 65536;
    policy.is_huge_function = code.size() > 131072;

    const size_t param_count = count_occurrences_limited(code, "param_", 128);
    const size_t sub_count = count_occurrences_limited(code, "sub_", 96);
    const size_t goto_count = count_occurrences_limited(code, "goto ", 256);
    const size_t label_count = count_occurrences_limited(code, "LAB_", 256);
    const size_t hex_count = count_occurrences_limited(code, "0x", 512);
    const size_t line_count = count_occurrences_limited(code, "\n", 8192);
    policy.has_stack_names =
        code.find("Stack") != std::string::npos ||
        code.find("stack") != std::string::npos;
    policy.is_giant_dispatcher =
        (code.size() > 98304 && goto_count > 32) ||
        (code.size() > 131072 && label_count > 64) ||
        (line_count > 2500 && goto_count > 24);
    policy.is_extreme_dispatcher =
        code.size() > 196608 ||
        (code.size() > 131072 && goto_count > 96) ||
        (label_count > 160 && goto_count > 48) ||
        (line_count > 4000 && goto_count > 32);
    policy.use_fast_postprocessor =
        policy.is_giant_dispatcher || policy.is_extreme_dispatcher;
    policy.skip_postprocessor =
        policy.is_extreme_dispatcher && goto_count > 128;
    policy.skip_smart_constant_replace =
        policy.is_extreme_dispatcher ||
        (policy.is_giant_dispatcher && hex_count > 160);
    policy.skip_constant_cleanup =
        policy.is_extreme_dispatcher ||
        (policy.is_giant_dispatcher && hex_count > 192);
    policy.skip_demangle_cleanup =
        policy.is_extreme_dispatcher ||
        (policy.is_giant_dispatcher && sub_count > 24);
    policy.skip_virtual_call_cleanup =
        policy.is_extreme_dispatcher ||
        (policy.is_giant_dispatcher && label_count > 64);

    policy.skip_standardize_variable_names =
        !policy.has_stack_names ||
        policy.use_fast_postprocessor ||
        (policy.is_large_function && param_count > 48);
    policy.skip_signature_cleanup =
        policy.use_fast_postprocessor ||
        policy.is_huge_function ||
        (policy.is_large_function && param_count > 64);
    policy.skip_internal_name_cleanup =
        policy.use_fast_postprocessor ||
        policy.is_huge_function ||
        (policy.is_large_function && sub_count > 32);
    policy.skip_struct_offset_annotation =
        analysis.type_replacements.empty() ? false :
        (policy.use_fast_postprocessor ||
         policy.is_huge_function ||
         (policy.is_large_function && analysis.type_replacements.size() > 48));
    policy.skip_fid_resolution =
        policy.use_fast_postprocessor ||
        policy.is_huge_function ||
        (policy.is_large_function && sub_count > 24);

    return policy;
}

}  // namespace

std::string run_post_processing(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    const std::string& code,
    const AnalysisArtifacts& analysis,
    const PostProcessOptions& options,
    fission::ffi::NativeDecompTiming* timing
) {
    if (!ctx) {
        return code;
    }

    std::string result = code;
    const PostProcessHotPathPolicy policy = build_postprocess_policy(result, analysis);

    if (options.apply_struct_definitions && !analysis.inferred_struct_definitions.empty()) {
        std::string with_structs = TypePropagator::apply_struct_types(
            result,
            fd,
            analysis.captured_structs
        );
        if (with_structs != result) {
            result = analysis.inferred_struct_definitions + "\n" + with_structs;
        }
    }

    // Step 1: IAT symbol replacement
    if (options.iat_symbols && (result.find("pcRam") != std::string::npos ||
                                result.find(".dll!") != std::string::npos)) {
        result = post_process_iat_calls(result, ctx->symbols);
    }

    // Step 2: Smart constant replacement
    if (!policy.skip_smart_constant_replace &&
        options.smart_constants &&
        result.find("0x") != std::string::npos) {
        auto smart_constant_start = std::chrono::steady_clock::now();
        result = smart_constant_replace(result);
        if (timing != nullptr) {
            timing->smart_constant_replace_ms += std::chrono::duration<double, std::milli>(
                std::chrono::steady_clock::now() - smart_constant_start
            ).count();
        }
    }

    // Step 2.5: String inlining
    if (options.inline_strings) {
        static const std::unordered_set<std::string> string_sections = {
            ".rdata",        // PE read-only data
            ".rodata",       // ELF read-only data
            "__cstring",     // Mach-O C-string literals
            "__const",       // Mach-O read-only constants
            ".data.rel.ro",  // ELF RELRO (relocated read-only)
        };

        // Build the section string table once per binary; reuse for every function.
        if (!ctx->string_table_built) {
            for (const auto& block : ctx->memory_blocks) {
                if (string_sections.count(block.name) == 0 || block.file_size == 0) continue;
                size_t start_idx = block.file_offset;
                size_t end_idx   = start_idx + block.file_size;
                if (end_idx > ctx->binary_data.size()) continue;
                std::vector<uint8_t> section_data(
                    ctx->binary_data.begin() + start_idx,
                    ctx->binary_data.begin() + end_idx
                );
                auto sec_strings = StringScanner::scan_ascii_strings(section_data, block.va_addr);
                ctx->cached_string_table.insert(sec_strings.begin(), sec_strings.end());
            }
            ctx->string_table_built = true;
        }

        if (!ctx->cached_string_table.empty() && result.find("0x") != std::string::npos) {
            result = inline_strings(result, ctx->cached_string_table);
        }
    }

    // Step 3: Constant replacement
    if (!policy.skip_constant_cleanup &&
        options.constants &&
        result.find("0x") != std::string::npos) {
        std::map<uint64_t, std::string> enum_values;
        result = post_process_constants(result, enum_values);
    }

    // Step 4: GUID substitution
    if (options.guids && result.find('-') != std::string::npos) {
        const auto& guid_map = get_guid_map();
        if (!guid_map.empty()) {
            result = substitute_guids(result, guid_map);
        }
    }

    // Step 5: Unicode string recovery
    if (options.unicode_strings) {
        result = recover_unicode_strings(result);
    }

    // Step 6: Interlocked pattern replacement
    if (options.interlocked_patterns && result.find("LOCK();") != std::string::npos) {
        result = replace_interlocked_patterns(result);
    }

    // Step 6.5: Variable naming standardization (Ghidra standard)
    if (!policy.skip_standardize_variable_names) {
        result = standardize_variable_names(result);
    }

    // Step 7: xunknown/undefined type replacement
    if (options.xunknown_types && (result.find("xunknown") != std::string::npos ||
                                   result.find("undefined") != std::string::npos)) {
        result = replace_xunknown_types(result);
    }

    // Step 8: SEH boilerplate cleanup
    if (options.seh_cleanup && (result.find("__try") != std::string::npos ||
                                result.find("__except") != std::string::npos ||
                                result.find("ExceptionList") != std::string::npos)) {
        result = cleanup_seh_boilerplate(result);
    }

    // Step 8.5: Apply global data symbol names (g_/gp_)
    if (options.global_symbols && !ctx->global_symbols.empty() &&
        (result.find("g_") != std::string::npos || result.find("gp_") != std::string::npos)) {
        result = apply_global_symbols(result, ctx->global_symbols);
    }

    // Step 9: Internal function naming improvement
    if (options.internal_names) {
        if (!policy.skip_demangle_cleanup) {
            result = demangle_cpp_names(result);
        }
        if (!policy.skip_virtual_call_cleanup &&
            (result.find("->") != std::string::npos || result.find("vtable") != std::string::npos)) {
            result = normalize_cpp_virtual_calls(
                result,
                ctx->vtable_virtual_names,
                ctx->vcall_slot_name_hints,
                ctx->vcall_slot_target_hints
            );
        }
        if (!policy.skip_signature_cleanup && result.find("param_") != std::string::npos) {
            result = apply_function_signatures(result);
        }
        if (result.find("__mingw_printf") != std::string::npos) {
            result = normalize_mingw_printf_args(result);
        }
        if (result.find("__stdio_common_v") != std::string::npos) {
            result = normalize_msvc_crt_printf(result);
        }
        if (!policy.skip_internal_name_cleanup) {
            result = improve_internal_function_names(result);
        }
    }

    // Step 9.5: Structure offset annotation
    if (options.struct_offsets && !policy.skip_struct_offset_annotation) {
        if (!analysis.type_replacements.empty()) {
            result = annotate_structure_offsets(result, analysis.type_replacements);
        } else {
            result = annotate_structure_offsets(result);
        }
    }

    // Step 10: Apply FID-resolved function names
    if (options.fid_names && !ctx->fid_databases.empty() && ctx->matcher &&
        !policy.skip_fid_resolution &&
        result.find("sub_") != std::string::npos) {
        std::map<uint64_t, std::string> fid_names;

        std::regex func_pattern(R"(sub_([0-9a-fA-F]{8,16}))");
        std::smatch match;
        std::string::const_iterator search_start(result.cbegin());
        std::set<uint64_t> found_addrs;

        while (std::regex_search(search_start, result.cend(), match, func_pattern)) {
            try {
                uint64_t func_addr = std::stoull(match[1].str(), nullptr, 16);
                found_addrs.insert(func_addr);
            } catch (...) {
                // Ignore parse errors
            }
            search_start = match.suffix().first;
        }

        int fid_matches = 0;
        for (uint64_t func_addr : found_addrs) {
            try {
                std::vector<uint8_t> code_bytes(64);
                if (!ctx->arch) {
                    continue;
                }
                ghidra::Address read_addr(ctx->arch->getDefaultCodeSpace(), func_addr);
                ctx->memory_image->loadFill(code_bytes.data(), 64, read_addr);

                uint64_t hash = FidHasher::calculate_full_hash(code_bytes.data(), code_bytes.size());

                bool found_match = false;
                for (size_t db_idx = 0; db_idx < ctx->fid_databases.size() && !found_match; ++db_idx) {
                    std::vector<std::string> names = ctx->fid_databases[db_idx]->lookup_by_hash(hash);
                    if (!names.empty()) {
                        fid_names[func_addr] = names[0];
                        fid_matches++;
                        found_match = true;
                    }
                }
            } catch (...) {
                // Ignore errors
            }
        }

        if (fid_matches > 0) {
            result = apply_fid_names(result, fid_names);
        }
    }

    // Step 11: Advanced Structurization and Cleanup (Fission Core Improvement)
    if (!policy.skip_postprocessor) {
        PostProcessorTrace trace;
        result = PostProcessor::process(result, policy.use_fast_postprocessor, &trace);
        if (timing != nullptr) {
            timing->cfg_structurizer_ms += trace.cfg_structurizer_ms;
            timing->loop_normalize_ms += trace.loop_normalize_ms;
        }
    }

    // Step 11.5: Strip Windows x64 MSVC shadow-spill parameters.
    if (options.strip_shadow_params) {
        result = strip_shadow_only_params(result);
    }

    return result;
}

} // namespace decompiler
} // namespace fission
