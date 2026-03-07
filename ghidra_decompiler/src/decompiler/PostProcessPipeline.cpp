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
#include <unordered_set>
#include <vector>
#include <chrono>
#include "fission/utils/logger.h"

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

std::string run_post_processing(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    const std::string& code,
    const AnalysisArtifacts& analysis,
    const PostProcessOptions& options
) {
    if (!ctx) {
        return code;
    }

    std::string result = code;

    auto pp_start = std::chrono::steady_clock::now();
    auto ms = [](auto a, auto b) { return std::chrono::duration<double, std::milli>(b - a).count(); };

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
    auto t1 = std::chrono::steady_clock::now();

    // Step 1: IAT symbol replacement
    if (options.iat_symbols) {
        result = post_process_iat_calls(result, ctx->symbols);
    }
    auto t2 = std::chrono::steady_clock::now();

    // Step 2: Smart constant replacement
    if (options.smart_constants) {
        result = smart_constant_replace(result);
    }
    auto t3 = std::chrono::steady_clock::now();

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

        if (!ctx->cached_string_table.empty()) {
            result = inline_strings(result, ctx->cached_string_table);
        }
    }
    auto t4 = std::chrono::steady_clock::now();

    // Step 3: Constant replacement
    if (options.constants) {
        std::map<uint64_t, std::string> enum_values;
        result = post_process_constants(result, enum_values);
    }
    auto t5 = std::chrono::steady_clock::now();

    // Step 4: GUID substitution
    if (options.guids) {
        const auto& guid_map = get_guid_map();
        if (!guid_map.empty()) {
            result = substitute_guids(result, guid_map);
        }
    }
    auto t6 = std::chrono::steady_clock::now();

    // Step 5: Unicode string recovery
    if (options.unicode_strings) {
        result = recover_unicode_strings(result);
    }

    // Step 6: Interlocked pattern replacement
    if (options.interlocked_patterns) {
        result = replace_interlocked_patterns(result);
    }

    // Step 6.5: Variable naming standardization (Ghidra standard)
    {
        result = standardize_variable_names(result);
    }
    auto t7 = std::chrono::steady_clock::now();

    // Step 7: xunknown/undefined type replacement
    if (options.xunknown_types) {
        result = replace_xunknown_types(result);
    }
    auto t8 = std::chrono::steady_clock::now();

    // Step 8: SEH boilerplate cleanup
    if (options.seh_cleanup) {
        result = cleanup_seh_boilerplate(result);
    }

    // Step 8.5: Apply global data symbol names (g_/gp_)
    if (options.global_symbols && !ctx->global_symbols.empty()) {
        result = apply_global_symbols(result, ctx->global_symbols);
    }

    // Step 9: Internal function naming improvement
    if (options.internal_names) {
        result = demangle_cpp_names(result);
        result = normalize_cpp_virtual_calls(
            result,
            ctx->vtable_virtual_names,
            ctx->vcall_slot_name_hints,
            ctx->vcall_slot_target_hints
        );
        result = apply_function_signatures(result);
        result = normalize_mingw_printf_args(result);
        result = normalize_msvc_crt_printf(result);
        result = improve_internal_function_names(result);
    }
    auto t9 = std::chrono::steady_clock::now();

    // Step 9.5: Structure offset annotation
    if (options.struct_offsets) {
        if (!analysis.type_replacements.empty()) {
            result = annotate_structure_offsets(result, analysis.type_replacements);
        } else {
            result = annotate_structure_offsets(result);
        }
    }
    auto t10 = std::chrono::steady_clock::now();

    // Step 10: Apply FID-resolved function names
    if (options.fid_names && !ctx->fid_databases.empty() && ctx->matcher) {
        std::map<uint64_t, std::string> fid_names;

        // Fast O(N) scan for "sub_" + hex digits (replaces slow std::regex)
        std::set<uint64_t> found_addrs;
        {
            const char* p = result.c_str();
            const char* end = p + result.size();
            while (p < end) {
                p = static_cast<const char*>(memchr(p, 's', end - p));
                if (!p) break;
                if (end - p >= 5 && p[1] == 'u' && p[2] == 'b' && p[3] == '_') {
                    const char* hex_start = p + 4;
                    const char* hp = hex_start;
                    while (hp < end && ((*hp >= '0' && *hp <= '9') ||
                                         (*hp >= 'a' && *hp <= 'f') ||
                                         (*hp >= 'A' && *hp <= 'F'))) {
                        ++hp;
                    }
                    size_t hex_len = hp - hex_start;
                    if (hex_len >= 8 && hex_len <= 16) {
                        char buf[17];
                        memcpy(buf, hex_start, hex_len);
                        buf[hex_len] = '\0';
                        char* ep = nullptr;
                        uint64_t func_addr = strtoull(buf, &ep, 16);
                        if (ep == buf + hex_len) {
                            found_addrs.insert(func_addr);
                        }
                    }
                    p = hp;
                } else {
                    ++p;
                }
            }
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
    auto t11 = std::chrono::steady_clock::now();

    // Step 11: Advanced Structurization and Cleanup (Fission Core Improvement)
    {
        result = PostProcessor::process(result);
    }
    auto t12 = std::chrono::steady_clock::now();

    // Step 11.5: Strip Windows x64 MSVC shadow-spill parameters.
    if (options.strip_shadow_params) {
        result = strip_shadow_only_params(result);
    }
    auto t13 = std::chrono::steady_clock::now();

    // ===== PP TIMING REPORT =====
    fission::utils::log_stream()
        << "[PP-PERF] struct=" << ms(pp_start, t1)
        << "  iat=" << ms(t1, t2)
        << "  smartconst=" << ms(t2, t3)
        << "  strings=" << ms(t3, t4)
        << "  const=" << ms(t4, t5)
        << "  guid=" << ms(t5, t6)
        << "  naming=" << ms(t6, t7)
        << "  xunknown=" << ms(t7, t8)
        << "  internal=" << ms(t8, t9)
        << "  struct_off=" << ms(t9, t10)
        << "  fid=" << ms(t10, t11)
        << "  postproc=" << ms(t11, t12)
        << "  shadow=" << ms(t12, t13)
        << "  TOTAL=" << ms(pp_start, t13) << "ms" << std::endl;

    return result;
}

} // namespace decompiler
} // namespace fission
