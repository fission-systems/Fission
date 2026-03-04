#include <cstring>
#include "fission/decompiler/DecompilationPipeline.h"
#include "fission/decompiler/AnalysisPipeline.h"
#include "fission/decompiler/Limits.h"
#include "fission/decompiler/PostProcessor.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/utils/json_utils.h"
#include "fission/utils/encoding.h"
#include "fission/utils/file_utils.h"
#include "fission/utils/StepTimer.h"
#include "fission/utils/logger.h"
#include "fission/loader/BinaryDetector.h"
#include "fission/loader/SymbolLoader.h"
#include "fission/loaders/DataSectionScanner.h"
#include "fission/types/RttiAnalyzer.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/PatternLoader.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/types/GuidParser.h"
#include "fission/processing/StringScanner.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/analysis/RelationValidator.h"
#include "fission/analysis/CallingConvDetector.h"
#include "fission/analysis/EnumInferrer.h"
#include "fission/analysis/VTableAnalyzer.h"
#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/analysis/CallGraphAnalyzer.h"
#include "fission/analysis/TypeSharing.h"
#include "fission/analysis/InternalMatcher.h"
#include "fission/analysis/EmulationAnalyzer.h"
#include "fission/config/PathConfig.h"
#include "fission/core/DataSymbolRegistry.h"
#include "fission/decompiler/PcodeOptimizationBridge.h"
#include "fission/decompiler/PcodeExtractor.h"
#include "libdecomp.hh"
#include "database.hh"
#include "type.hh"
#include <iostream>
#include <sstream>
#include <iomanip>
#include <set>
#include <unordered_set>

using namespace fission::utils;
using namespace fission::loader;
using namespace fission::types;
using namespace fission::processing;
using namespace fission::analysis;

namespace fission {
namespace decompiler {

// Budget guards for structure recovery
// Raised from 10000/100 to better handle large real-world functions.
// A chrono-based timeout inside collect_accesses() provides the true
// wall-clock safety net, so these are secondary hard caps.
static constexpr size_t MAX_FUNCTION_SIZE = 50000;  // 50KB code limit
static constexpr int MAX_PTRSUB_OPS = 500;           // Limit analyzed operations

// ============================================================================
// Centralized FID/Signature Path Configuration
// ============================================================================

using namespace fission::config;

// Helper: Get all available FID paths for a given architecture
static std::vector<std::string> get_all_fid_paths_wrapper(bool is_64bit) {
    return ::fission::config::get_all_fid_paths(is_64bit);
}

static std::vector<std::string> collect_referenced_strings_near(
    const std::vector<uint8_t>& bytes,
    size_t func_off,
    uint64_t image_base,
    const std::map<uint64_t, std::string>& known_strings,
    ArchType arch = ArchType::X86_64   // x86 byte patterns only — skip for ARM/ARM64
) {
    std::vector<std::string> refs;

    // ── ARM64 path: ADRP + ADD/LDR instruction pairs ──────────────────────
    if (arch == ArchType::ARM64) {
        if (known_strings.empty() || func_off >= bytes.size()) return refs;

        std::set<std::string> unique;
        auto add_string_at = [&](uint64_t addr) {
            auto it = known_strings.find(addr);
            if (it != known_strings.end() && !it->second.empty()) {
                unique.insert(it->second);
            }
        };

        // ARM64 instructions are 4-byte aligned
        size_t start = func_off & ~(size_t)3;
        const size_t window_end = std::min(bytes.size(), start + 0x200);

        // Track last ADRP result per destination register (0-31)
        uint64_t adrp_page[32] = {};
        bool     adrp_valid[32] = {};
        std::memset(adrp_valid, 0, sizeof(adrp_valid));

        for (size_t p = start; p + 4 <= window_end; p += 4) {
            uint32_t insn = 0;
            std::memcpy(&insn, &bytes[p], sizeof(insn));

            uint64_t pc = image_base + p;

            // ADRP Xd, #page — encoding: [1] immlo[30:29] 10000 immhi[23:5] Rd[4:0]
            if ((insn & 0x9F000000u) == 0x90000000u) {
                uint32_t rd    = insn & 0x1Fu;
                int32_t  immhi = (int32_t)((insn >> 5) & 0x7FFFFu);
                uint32_t immlo = (insn >> 29) & 0x3u;
                // Sign-extend immhi from 19 bits
                if (immhi & 0x40000) immhi |= (int32_t)0xFFF80000;
                int64_t imm = ((int64_t)immhi << 2) | (int64_t)immlo;
                uint64_t page = (pc & ~(uint64_t)0xFFF) + ((uint64_t)imm << 12);
                adrp_page[rd]  = page;
                adrp_valid[rd] = true;
                continue;
            }

            // ADD Xd, Xn, #imm12 — encoding: 1001000100 shift[23:22] imm12[21:10] Rn[9:5] Rd[4:0]
            if ((insn & 0xFFC00000u) == 0x91000000u) {
                uint32_t rd   = insn & 0x1Fu;
                uint32_t rn   = (insn >> 5) & 0x1Fu;
                uint32_t imm12 = (insn >> 10) & 0xFFFu;
                uint32_t shift = (insn >> 22) & 0x3u;
                if (shift == 1) imm12 <<= 12;  // LSL #12
                if (adrp_valid[rn]) {
                    uint64_t target = adrp_page[rn] + imm12;
                    add_string_at(target);
                }
                // Track the result in rd as well (ADD overwrites rd)
                if (adrp_valid[rn]) {
                    adrp_page[rd]  = adrp_page[rn] + imm12;
                    adrp_valid[rd] = true;
                }
                continue;
            }

            // LDR Xt, [Xn, #imm12*8] (unsigned offset, 64-bit)
            // encoding: 11 111 0 01 01 imm12[21:10] Rn[9:5] Rt[4:0]
            if ((insn & 0xFFC00000u) == 0xF9400000u) {
                uint32_t rn    = (insn >> 5) & 0x1Fu;
                uint32_t imm12 = (insn >> 10) & 0xFFFu;
                if (adrp_valid[rn]) {
                    uint64_t target = adrp_page[rn] + (imm12 << 3);
                    add_string_at(target);
                }
                continue;
            }

            // LDR Wt, [Xn, #imm12*4] (unsigned offset, 32-bit)
            // encoding: 10 111 0 01 01 imm12[21:10] Rn[9:5] Rt[4:0]
            if ((insn & 0xFFC00000u) == 0xB9400000u) {
                uint32_t rn    = (insn >> 5) & 0x1Fu;
                uint32_t imm12 = (insn >> 10) & 0xFFFu;
                if (adrp_valid[rn]) {
                    uint64_t target = adrp_page[rn] + (imm12 << 2);
                    add_string_at(target);
                }
                continue;
            }
        }

        refs.assign(unique.begin(), unique.end());
        return refs;
    }

    // ── x86/x64 path: instruction byte patterns ──────────────────────────
    // The patterns below are exclusively x86/x64 instruction encodings.
    // Returning an empty set for other architectures avoids false positives.
    if (arch != ArchType::X86 && arch != ArchType::X86_64) {
        return refs;
    }
    if (known_strings.empty() || func_off >= bytes.size()) {
        return refs;
    }

    std::set<std::string> unique;
    auto add_string_at = [&](uint64_t addr) {
        auto it = known_strings.find(addr);
        if (it != known_strings.end() && !it->second.empty()) {
            unique.insert(it->second);
        }
    };

    const size_t window_end = std::min(bytes.size(), func_off + 0x120);
    for (size_t p = func_off; p + 6 < window_end; ++p) {
        // x64 RIP-relative LEA/MOV forms:
        // 48 8D/8B ?? disp32, 4C 8D/8B ?? disp32 with modrm=00 r/m=101 (0x05)
        if ((bytes[p] == 0x48 || bytes[p] == 0x4C) &&
            (bytes[p + 1] == 0x8D || bytes[p + 1] == 0x8B) &&
            ((bytes[p + 2] & 0x07) == 0x05)) {
            int32_t disp = 0;
            std::memcpy(&disp, &bytes[p + 3], sizeof(disp));
            uint64_t insn_end = image_base + static_cast<uint64_t>(p + 7);
            uint64_t target = static_cast<uint64_t>(static_cast<int64_t>(insn_end) + disp);
            add_string_at(target);
            continue;
        }

        // x86 absolute LEA: 8D 05 imm32
        if (bytes[p] == 0x8D && bytes[p + 1] == 0x05) {
            uint32_t imm = 0;
            std::memcpy(&imm, &bytes[p + 2], sizeof(imm));
            add_string_at(static_cast<uint64_t>(imm));
            continue;
        }

        // push imm32 (often used for string arguments in x86)
        if (bytes[p] == 0x68 && p + 4 < window_end) {
            uint32_t imm = 0;
            std::memcpy(&imm, &bytes[p + 1], sizeof(imm));
            add_string_at(static_cast<uint64_t>(imm));
            continue;
        }

        // mov r32, imm32 (B8..BF)
        if (bytes[p] >= 0xB8 && bytes[p] <= 0xBF && p + 4 < window_end) {
            uint32_t imm = 0;
            std::memcpy(&imm, &bytes[p + 1], sizeof(imm));
            add_string_at(static_cast<uint64_t>(imm));
            continue;
        }
    }

    refs.assign(unique.begin(), unique.end());
    return refs;
}

// ============================================================================
// Parse a Ghidra sleigh_id string ("ARCH:ENDIAN:BITS:VARIANT") into BinaryInfo.
//
// Steps:
//  1. Derive arch type + bitness from the sleigh_id fields.
//  2. Try BinaryDetector::detect() on the raw bytes for format (most accurate).
//  3. If bytes are absent or format is UNKNOWN, fall back to compiler_id hints.
// ============================================================================
static BinaryInfo parse_sleigh_id(const std::string& sleigh_id,
                                   const std::string& compiler_id,
                                   const std::vector<uint8_t>& bytes) {
    BinaryInfo info;
    info.sleigh_id  = sleigh_id;
    info.compiler_id = compiler_id.empty() ? "default" : compiler_id;

    // Split sleigh_id on ':' to read individual fields
    std::vector<std::string> fields;
    {
        std::istringstream ss(sleigh_id);
        std::string tok;
        while (std::getline(ss, tok, ':')) fields.push_back(tok);
    }
    const std::string arch_field = fields.size() > 0 ? fields[0] : "";
    const std::string bits_field = fields.size() > 2 ? fields[2] : "";

    // Determine bitness from the third field ("32" or "64")
    info.is_64bit = (bits_field == "64");

    // Determine ArchType from the first field
    if (arch_field == "x86") {
        info.arch = info.is_64bit ? ArchType::X86_64 : ArchType::X86;
    } else if (arch_field == "AARCH64") {
        info.arch   = ArchType::ARM64;
        info.is_64bit = true;  // AARCH64 is always 64-bit
    } else if (arch_field == "ARM") {
        info.arch   = ArchType::ARM;
        info.is_64bit = false;
    }
    // else: leave arch = UNKNOWN

    // Try to detect format from binary magic bytes (most accurate)
    if (!bytes.empty()) {
        BinaryInfo detected = BinaryDetector::detect(bytes.data(), bytes.size());
        if (detected.format != BinaryFormat::UNKNOWN) {
            info.format = detected.format;
            // Trust the byte-level bitness only when sleigh_id did not specify it
            if (bits_field.empty()) info.is_64bit = detected.is_64bit;
            return info;
        }
    }

    // Fall back to compiler_id heuristics
    if (info.compiler_id == "windows") {
        info.format = BinaryFormat::PE;
    } else if (info.compiler_id == "gcc") {
        info.format = BinaryFormat::ELF;
    } else if (info.compiler_id == "clang") {
        // clang targets both ELF (Linux) and Mach-O (Apple); pick by arch
        info.format = (info.arch == ArchType::ARM64) ? BinaryFormat::MACHO : BinaryFormat::ELF;
    }
    // else: format stays UNKNOWN (caller will warn)

    return info;
}

std::string DecompilationPipeline::process_request(
    core::DecompilerContext& state, 
    const std::string& input
) {
    fission::utils::log_stream() << "[fission_decomp] Received request: " 
              << input.substr(0, std::min(input.size(), (size_t)100)) << "..." << std::endl;
    
    // Check for special commands
    std::string cmd = extract_json_string(input, "cmd");
    if (cmd == "quit") {
        return "__QUIT__";
    }
    if (cmd == "ping") {
        return "{\"status\":\"ok\",\"message\":\"pong\"}";
    }
    
    try {
        // Detect command type
        std::string load_bin_cmd = extract_json_string(input, "load_bin");
        if (!load_bin_cmd.empty()) {
            return handle_load_bin(state, input);
        } else {
            return handle_decompile(state, input);
        }
    } catch (const ghidra::LowlevelError& e) {
        return "{\"status\":\"error\",\"message\":\"" + json_escape(e.explain) + "\"}";
    } catch (const std::exception& e) {
        return "{\"status\":\"error\",\"message\":\"" + json_escape(e.what()) + "\"}";
    } catch (...) {
        return "{\"status\":\"error\",\"message\":\"Unknown Ghidra error\"}";
    }
}

// ============================================================================
// handle_load_bin sub-phases
// ============================================================================

BinaryInfo DecompilationPipeline::detect_binary_info(
    const std::string& input,
    const std::vector<uint8_t>& bytes
) {
    std::string req_sleigh_id  = extract_json_string(input, "sleigh_id");
    std::string req_compiler_id = extract_json_string(input, "compiler_id");

    BinaryInfo info;
    if (!req_sleigh_id.empty()) {
        fission::utils::log_stream() << "[fission_decomp] Using provided sleigh_id: "
                  << req_sleigh_id << std::endl;
        info = parse_sleigh_id(req_sleigh_id, req_compiler_id, bytes);
    } else {
        fission::utils::log_stream() << "[fission_decomp] Debug: Detecting Binary Format..."
                  << std::endl;
        info = BinaryDetector::detect(bytes.data(), bytes.size());
    }

    bool is_pe    = (info.format == BinaryFormat::PE);
    bool is_elf   = (info.format == BinaryFormat::ELF);
    bool is_macho = (info.format == BinaryFormat::MACHO);

    if (info.format != BinaryFormat::UNKNOWN || !info.sleigh_id.empty()) {
        std::string compiler_id = info.compiler_id.empty() ? "default"
                                                           : info.compiler_id;
        fission::utils::log_stream() << "[fission_decomp] Binary Info: "
                  << (is_pe ? "PE" : (is_elf ? "ELF"
                                     : (is_macho ? "Mach-O" : "Unknown")))
                  << " " << (info.is_64bit ? "64-bit" : "32-bit")
                  << " Arch=" << info.sleigh_id
                  << " Compiler=" << compiler_id << std::endl;
    } else {
        fission::utils::log_stream()
            << "[fission_decomp] WARNING: Binary format undetected "
            << "(format=UNKNOWN, sleigh_id empty). "
            << "Falling back to PE x64/windows. "
            << "Pass a valid binary or supply sleigh_id in the request."
            << std::endl;
        info.format    = BinaryFormat::PE;
        info.is_64bit  = true;
        info.sleigh_id = "x86:LE:64:default";
        info.compiler_id = "windows";
    }

    if (info.compiler_id.empty()) {
        info.compiler_id = "default";
    }

    return info;
}

void DecompilationPipeline::run_preanalysis(
    core::DecompilerContext& state,
    const BinaryInfo& bin_info,
    const std::vector<uint8_t>& bin_bytes,
    uint64_t image_base,
    const std::string& compiler_id
) {
    bool is_pe    = (bin_info.format == BinaryFormat::PE);
    bool is_elf   = (bin_info.format == BinaryFormat::ELF);
    bool is_macho = (bin_info.format == BinaryFormat::MACHO);

    if (!state.arch_64bit_ready) {
        // Safety: delete if exists
        if (state.loader_64bit) delete state.loader_64bit;
        if (state.arch_64bit) delete state.arch_64bit;

        // Phase 3: RTTI Recovery
        std::map<uint64_t, std::string> recovered_classes;
        if (is_pe || is_elf || is_macho) {
            fission::utils::log_stream() << "[fission_decomp] Debug: Running RTTI Recovery..."
                << std::endl;
            recovered_classes = RttiAnalyzer::recover_class_names(
                bin_bytes, image_base, bin_info.is_64bit);
            if (!recovered_classes.empty()) {
                fission::utils::log_stream() << "[fission_decomp] Recovered "
                    << recovered_classes.size()
                    << " class names via RTTI." << std::endl;
            }
        }

        // Phase 4: VTable Analysis
        VTableAnalyzer vtable_analyzer;
        vtable_analyzer.scan_vtables(bin_bytes.data(), bin_bytes.size(),
            image_base, bin_info.is_64bit);
        if (!recovered_classes.empty()) {
            vtable_analyzer.link_with_rtti(recovered_classes);
        }

        // Build reusable virtual-call display maps for per-function post-processing
        state.vtable_virtual_names.clear();
        state.vcall_slot_name_hints.clear();
        state.vcall_slot_target_hints.clear();
        const int ptr_size = bin_info.is_64bit ? 8 : 4;
        std::set<int> ambiguous_slot_targets;
        const auto is_unresolved_vname = [](const std::string& name) {
            return name.empty() ||
                   name.find("::vfunc_") != std::string::npos ||
                   name.rfind("sub_", 0) == 0;
        };
        for (const auto& vt : vtable_analyzer.get_vtables()) {
            for (size_t i = 0; i < vt.entries.size(); ++i) {
                int slot_offset = static_cast<int>(i) * ptr_size;
                std::string display_name = vtable_analyzer.get_virtual_call_name(
                    vt.address, slot_offset, ptr_size);
                uint64_t resolved_target = vtable_analyzer.resolve_virtual_call(
                    vt.address, slot_offset, ptr_size);

                if (resolved_target != 0) {
                    if (!ambiguous_slot_targets.count(slot_offset)) {
                        auto it_slot_target =
                            state.vcall_slot_target_hints.find(slot_offset);
                        if (it_slot_target == state.vcall_slot_target_hints.end()) {
                            state.vcall_slot_target_hints[slot_offset] = resolved_target;
                        } else if (it_slot_target->second != resolved_target) {
                            state.vcall_slot_target_hints.erase(it_slot_target);
                            ambiguous_slot_targets.insert(slot_offset);
                        }
                    }
                }

                // Fallback to resolved call target if class/slot name is generic.
                if (is_unresolved_vname(display_name)) {
                    if (resolved_target != 0) {
                        auto it_iat = state.iat_symbols.find(resolved_target);
                        if (it_iat != state.iat_symbols.end()) {
                            display_name = it_iat->second;
                        } else {
                            std::ostringstream ss;
                            ss << "sub_" << std::hex << resolved_target;
                            display_name = ss.str();
                        }
                    }
                }

                if (display_name.empty()) {
                    continue;
                }

                state.vtable_virtual_names[vt.address][slot_offset] = display_name;

                // Prefer RTTI-linked class names over generic ::vfunc_N placeholders.
                if (!is_unresolved_vname(display_name)) {
                    if (!state.vcall_slot_name_hints.count(slot_offset)) {
                        state.vcall_slot_name_hints[slot_offset] = display_name;
                    }
                }
            }
        }

        fission::utils::log_stream() << "[fission_decomp] VTable scan complete: "
            << vtable_analyzer.get_vtables().size() << " vtables found" << std::endl;
        fission::utils::log_stream() << "[fission_decomp] Virtual-call naming map: "
            << state.vtable_virtual_names.size() << " vtables, "
            << state.vcall_slot_name_hints.size() << " slot hints" << std::endl;

        // Phase 5: Global Data Analyzer
        GlobalDataAnalyzer global_analyzer;
        // C-1: Prefer first non-executable section from the parsed section table.
        uint64_t data_start = image_base + (bin_bytes.size() / 2);
        uint64_t data_end   = image_base + bin_bytes.size();
        for (const auto& sec : bin_info.sections) {
            if (!sec.is_executable && sec.virtual_size > 0) {
                data_start = sec.virtual_address;
                data_end   = sec.virtual_address + sec.virtual_size;
                break;
            }
        }
        global_analyzer.set_data_section(data_start, data_end);
        state.data_section_start = data_start;
        state.data_section_end = data_end;

        // Stash executable ranges for callgraph reanalysis in handle_decompile().
        state.executable_ranges.clear();
        for (const auto& sec : bin_info.sections) {
            if (sec.is_executable && sec.virtual_size > 0) {
                state.executable_ranges.emplace_back(
                    sec.virtual_address, sec.virtual_address + sec.virtual_size);
            }
        }

        // Phase 6: Pattern Matching
        fission::utils::log_stream() << "[fission_decomp] Debug: Loading Patterns..."
            << std::endl;
        auto patterns = PatternLoader::load_standard_patterns();

        fission::utils::log_stream() << "[fission_decomp] Debug: Running Pattern Matching..."
            << std::endl;
        auto matches = PatternLoader::match_functions(bin_bytes, image_base, patterns);
        if (!matches.empty()) {
            fission::utils::log_stream() << "[fission_decomp] Identified " << matches.size()
                << " standard library functions via patterns." << std::endl;
            state.iat_symbols.insert(matches.begin(), matches.end());
        }

        // Phase 7: String Scanning (pre-scan for matcher + later substitutions)
        fission::utils::log_stream() << "[fission_decomp] Debug: String Scanning..."
            << std::endl;
        auto ascii_strings = StringScanner::scan_ascii_strings(bin_bytes, image_base);
        auto unicode_strings = StringScanner::scan_unicode_strings(bin_bytes, image_base);
        std::map<uint64_t, std::string> scanned_strings = ascii_strings;
        scanned_strings.insert(unicode_strings.begin(), unicode_strings.end());

        fission::utils::log_stream() << "[fission_decomp] Scanned " << ascii_strings.size()
            << " ASCII and " << unicode_strings.size() << " Unicode strings." << std::endl;

        // Feed discovered strings into constant substitution maps
        state.enum_values.insert(ascii_strings.begin(), ascii_strings.end());
        state.enum_values.insert(unicode_strings.begin(), unicode_strings.end());

        // Phase 9: Setup Architecture
        fission::utils::log_stream() << "[fission_decomp] Debug: Creating Loader and Arch..."
            << std::endl;
        state.setup_architecture(true, bin_bytes, image_base, compiler_id,
            bin_info.sleigh_id);

        // Phase 9.5: Scan data sections for floating-point constants
        // Format-independent: works for PE (.rdata/.data), ELF (.rodata), Mach-O (__DATA)
        fission::utils::log_stream()
            << "[fission_decomp] Debug: Scanning data sections for constants..."
            << std::endl;

        int total_data_symbols = 0;
        {
            static const std::unordered_set<std::string> data_section_names = {
                ".rdata",        // PE read-only data
                ".data",         // PE/ELF writable data
                ".rodata",       // ELF read-only data
                "__const",       // Mach-O read-only constants (section name)
                "__DATA",        // Mach-O data segment
                ".data.rel.ro",  // ELF RELRO
            };

            fission::loaders::DataSectionScanner scanner;
            for (const auto& sec : bin_info.sections) {
                if (data_section_names.count(sec.name) == 0) continue;
                if (sec.file_size == 0 || sec.file_offset + sec.file_size > bin_bytes.size()) continue;

                fission::utils::log_stream()
                    << "[fission_decomp] Scanning section: " << sec.name
                    << " VA=0x" << std::hex << sec.virtual_address
                    << " file_offset=0x" << sec.file_offset
                    << " size=" << std::dec << sec.file_size << std::endl;

                auto symbols = scanner.scanDataSection(
                    bin_bytes.data() + sec.file_offset,
                    sec.virtual_address,
                    sec.file_size
                );

                total_data_symbols += core::registerDataSymbolsInGlobalScope(
                    state.arch_64bit, symbols,
                    [&](const loaders::DataSymbol& sym) {
                        core::DecompilerContext::DataSymbolInfo info;
                        info.name      = sym.name;
                        info.size      = sym.size;
                        info.type_meta = sym.type_meta;
                        state.data_section_symbols[sym.address] = info;
                    }
                );
            }
        }

        state.data_symbols_scanned = true;
        fission::utils::log_stream() << "[fission_decomp] Registered "
            << total_data_symbols
            << " data section symbols (cached for future use)" << std::endl;

        // P2-A: Register vtable type structures into Ghidra's TypeFactory
        vtable_analyzer.register_vtable_types(state.arch_64bit);
        fission::utils::log_stream()
            << "[fission_decomp] VTable types registered in TypeFactory."
            << std::endl;

    } else {
        state.loader_64bit->updateData(bin_bytes, image_base);
        state.arch_64bit->symboltab->getGlobalScope()->clear();
    }
}

void DecompilationPipeline::run_signature_analysis(
    core::DecompilerContext& state,
    const BinaryInfo& bin_info,
    const std::vector<uint8_t>& bin_bytes,
    uint64_t image_base,
    const std::string& compiler_id,
    const std::string& input
) {
    // Phase 10: Inject IAT symbols for 64-bit
    auto iat_symbols = extract_iat_symbols(input);
    fission::utils::log_stream() << "[fission_decomp] Parsed " << iat_symbols.size()
        << " IAT symbols from JSON" << std::endl;
    state.arch_64bit->injectIatSymbols(iat_symbols);
    state.iat_symbols = iat_symbols;

    // Phase 11: Setup 32-bit Architecture
    if (!state.arch_32bit_ready) {
        if (!bin_info.is_64bit) {
            state.setup_architecture(false, bin_bytes, image_base, compiler_id,
                bin_info.sleigh_id);
        }
    } else {
        state.loader_32bit->updateData(bin_bytes, image_base);
        state.arch_32bit->symboltab->getGlobalScope()->clear();
    }

    // Inject IAT symbols for 32-bit (only when arch is live)
    if (state.arch_32bit_ready) {
        state.arch_32bit->injectIatSymbols(iat_symbols);
    }

    // Phase 12: Unified prologue scan — InternalMatcher + multi-DB FID
    {
        // Load/reload per-context FID databases when arch changes
        if (!state.batch_fid_dbs_loaded ||
            state.batch_fid_dbs_is64bit != bin_info.is_64bit)
        {
            state.batch_fid_dbs.clear();
            std::vector<std::string> all_fid_paths =
                ::fission::config::get_all_fid_paths(bin_info.is_64bit);
            for (const auto& path : all_fid_paths) {
                FidDatabase db;
                if (db.load(path)) {
                    state.batch_fid_dbs.push_back(std::move(db));
                }
            }
            state.batch_fid_dbs_loaded = true;
            state.batch_fid_dbs_is64bit = bin_info.is_64bit;
            fission::utils::log_stream() << "[fission_decomp] Loaded "
                << state.batch_fid_dbs.size() << " FID databases total"
                << std::endl;
        }

        // Pass 1: step=1 prologue scan with comprehensive patterns
        InternalMatcher internal_matcher;
        int internal_matches_found = 0;
        std::vector<uint64_t> prologue_candidates;
        std::map<uint64_t, uint64_t> addr_to_hash;

        fission::utils::log_stream() << "[fission_decomp] Starting unified prologue scan on "
            << bin_bytes.size() << " bytes..." << std::endl;

        for (size_t i = 0; i + 32 < bin_bytes.size(); ++i) {
            const uint8_t b0 = bin_bytes[i];
            const uint8_t b1 = bin_bytes[i + 1];
            const uint8_t b2 = bin_bytes[i + 2];
            const uint8_t b3 = bin_bytes[i + 3];
            bool possible_start = false;

            if (bin_info.is_64bit) {
                if (b0 == 0x40 && b1 >= 0x50 && b1 <= 0x57) possible_start = true;
                if (b0 == 0x41 && b1 >= 0x50 && b1 <= 0x57) possible_start = true;
                if (b0 == 0x48 && b1 == 0x83 && b2 == 0xEC) possible_start = true;
                if (b0 == 0x48 && b1 == 0x81 && b2 == 0xEC) possible_start = true;
                if (b0 == 0x48 && b1 == 0x89) possible_start = true;
                if (b0 == 0x4C && b1 == 0x89) possible_start = true;
                if (b0 >= 0xB8 && b0 <= 0xBF) possible_start = true;
                if ((b0 == 0x31 || b0 == 0x33) && b1 == 0xC0) possible_start = true;
                if (b0 == 0x48 && b1 == 0x85 && b2 >= 0xC0 && b2 <= 0xFF)
                    possible_start = true;
                if (b0 == 0x55 && b1 == 0x48 && b2 == 0x89 && b3 == 0xE5)
                    possible_start = true;
            } else {
                if (b0 == 0x55 && b1 == 0x8B && b2 == 0xEC) possible_start = true;
                if (b0 == 0x55 && b1 == 0x89 && b2 == 0xE5) possible_start = true;
                if (b0 == 0x83 && b1 == 0xEC) possible_start = true;
                if (b0 == 0x81 && b1 == 0xEC) possible_start = true;
                if (b0 >= 0x50 && b0 <= 0x57) possible_start = true;
                if (b0 >= 0xB8 && b0 <= 0xBF) possible_start = true;
                if ((b0 == 0x31 || b0 == 0x33) && b1 == 0xC0) possible_start = true;
                if (b0 == 0x8B && b1 == 0x44 && b2 == 0x24) possible_start = true;
            }

            if (!possible_start) continue;

            const uint64_t addr = image_base + i;
            prologue_candidates.push_back(addr);

            // Pre-compute full FID hash for this candidate
            const size_t hash_len = std::min((size_t)64, bin_bytes.size() - i);
            if (hash_len >= 8) {
                addr_to_hash[addr] =
                    FidHasher::calculate_full_hash(&bin_bytes[i], hash_len);
            }

            // InternalMatcher: prologue-based
            if (!state.iat_symbols.count(addr)) {
                const int plen = static_cast<int>(
                    std::min<size_t>(16, bin_bytes.size() - i));
                const std::string iname =
                    internal_matcher.match_by_prologue(addr, &bin_bytes[i], plen);
                if (!iname.empty()) {
                    state.iat_symbols[addr] = iname;
                    ++internal_matches_found;
                }
            }
        }

        fission::utils::log_stream() << "[fission_decomp] Found "
            << prologue_candidates.size() << " prologue candidates." << std::endl;

        // Pass 2: Multi-DB FID hash matching with RelationValidator
        size_t matched_count = 0;
        for (const uint64_t addr : prologue_candidates) {
            if (state.iat_symbols.count(addr) ||
                state.fid_function_names.count(addr))
                continue;

            const auto hit = addr_to_hash.find(addr);
            if (hit == addr_to_hash.end() || hit->second == 0) continue;
            const uint64_t func_hash = hit->second;

            // Collect candidates across all loaded databases
            std::vector<const FidFunctionRecord*> all_candidates;
            FidDatabase* best_db = nullptr;
            for (auto& db : state.batch_fid_dbs) {
                auto cands = db.lookup_records_by_hash(func_hash);
                if (!cands.empty()) {
                    all_candidates.insert(all_candidates.end(),
                        cands.begin(), cands.end());
                    if (!best_db) best_db = &db;
                }
            }

            if (all_candidates.empty()) continue;

            if (all_candidates.size() == 1) {
                state.fid_function_names[addr] = all_candidates[0]->name;
                ++matched_count;
            } else if (best_db) {
                RelationValidator validator(
                    std::shared_ptr<FidDatabase>(best_db, [](FidDatabase*){}));

                // Collect callee hashes from CALL rel32 instructions
                std::vector<uint64_t> actual_callees;
                const size_t off = addr - image_base;
                for (size_t k = 0;
                     k < 0x100 && (off + k + 5) < bin_bytes.size(); ++k)
                {
                    if (bin_bytes[off + k] == 0xE8) {
                        int32_t rel = 0;
                        std::memcpy(&rel, &bin_bytes[off + k + 1], sizeof(rel));
                        const uint64_t target =
                            addr + k + 5 + static_cast<int64_t>(rel);
                        const auto th = addr_to_hash.find(target);
                        if (th != addr_to_hash.end())
                            actual_callees.push_back(th->second);
                    }
                    if (bin_bytes[off + k] == 0xC3) break;
                }

                const auto result =
                    validator.find_best_match(all_candidates, actual_callees);
                if (!result.name.empty()) {
                    state.fid_function_names[addr] = result.name;
                    ++matched_count;
                } else {
                    state.fid_function_names[addr] = all_candidates[0]->name;
                    ++matched_count;
                }
            }
        }

        fission::utils::log_stream() << "[fission_decomp] Unified scan complete: "
            << matched_count << " FID matches, "
            << internal_matches_found << " InternalMatcher matches" << std::endl;

        // Load common symbol files
        const auto symbol_files = ::fission::config::get_common_symbol_files();
        for (const auto& path : symbol_files) {
            if (file_exists(path)) {
                auto symbols = SymbolLoader::load_symbols_text(path);
                for (const auto& [a, n] : symbols) {
                    state.fid_function_names[a] = n;
                }
                fission::utils::log_stream()
                    << "[fission_decomp] Loaded common symbols from: " << path
                    << std::endl;
            }
        }
    }
}

// ============================================================================
// handle_load_bin — orchestrator that calls the extracted sub-phases
// ============================================================================

std::string DecompilationPipeline::handle_load_bin(
    core::DecompilerContext& state,
    const std::string& input
) {
    std::string load_bin_cmd = extract_json_string(input, "load_bin");
    std::vector<uint8_t> bin_bytes = base64_decode(load_bin_cmd);
    if (bin_bytes.empty()) {
        return "{\"status\":\"error\",\"message\":\"Failed to decode load_bin bytes\"}";
    }
    
    // Parse critical parameters
    int64_t image_base = extract_json_int(input, "image_base");
    std::string sla_dir = extract_json_string(input, "sla_dir");
    
    fission::utils::log_stream() << "[fission_decomp] load_bin: size=" << bin_bytes.size() 
              << " image_base=0x" << std::hex << image_base << std::dec << std::endl;
    
    // Debug: Print first 16 bytes
    fission::utils::log_stream() << "[fission_decomp] First 16 bytes: ";
    for (size_t i = 0; i < std::min((size_t)16, bin_bytes.size()); ++i) {
        fission::utils::log_stream() << std::hex << std::setw(2) << std::setfill('0') 
                  << (int)bin_bytes[i] << " ";
    }
    fission::utils::log_stream() << std::dec << std::endl;
    
    // Initialize Ghidra if needed
    if (sla_dir.empty()) {
        return "{\"status\":\"error\",\"message\":\"Missing sla_dir for load_bin\"}";
    }
    if (!state.initialize(sla_dir)) {
        return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
    }
    
    // Parse optional architecture/compiler info to avoid redundant detection
    // Phase 1: Binary Format Detection
    BinaryInfo bin_info = detect_binary_info(input, bin_bytes);
    std::string compiler_id = bin_info.compiler_id;

    // Phases 3-9.5: RTTI, VTable, GlobalData, Pattern, String, ArchSetup, DataSymbols
    run_preanalysis(state, bin_info, bin_bytes, image_base, compiler_id);

    // Phases 10-12: IAT injection, 32-bit arch, prologue/FID scan
    run_signature_analysis(state, bin_info, bin_bytes, image_base, compiler_id, input);

    return "{\"status\":\"ok\",\"message\":\"Binary loaded\"}";
}

std::string DecompilationPipeline::handle_decompile(
    core::DecompilerContext& state,
    const std::string& input
) {
    // Parse JSON input
    std::string bytes_b64 = extract_json_string(input, "bytes");
    int64_t address = extract_json_int(input, "address");
    bool is_64bit = extract_json_bool(input, "is_64bit");
    std::string sla_dir = extract_json_string(input, "sla_dir");
    std::string compiler_id = extract_json_string(input, "compiler_id");
    std::string sleigh_id   = extract_json_string(input, "sleigh_id");
    if (compiler_id.empty()) {
        compiler_id = "windows";
    }
    
    if (sla_dir.empty()) {
        return "{\"status\":\"error\",\"message\":\"Missing sla_dir\"}";
    }
    
    if (!state.initialize(sla_dir)) {
        return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
    }
    
    // Decode bytes
    std::vector<uint8_t> bytes;
    if (!bytes_b64.empty()) {
        bytes = base64_decode(bytes_b64);
    }
    
    // Setup architecture
    state.setup_architecture(is_64bit, bytes, address, compiler_id, sleigh_id);
    
    // Determine loader/arch
    MemoryLoadImage* loader = nullptr;
    core::CliArchitecture* arch = nullptr;
    
    if (is_64bit) {
        loader = state.loader_64bit;
        arch = state.arch_64bit;
    } else {
        loader = state.loader_32bit;
        arch = state.arch_32bit;
    }
    
    if (!bytes.empty()) {
        loader->updateData(bytes, address);
    }
    
    fission::utils::log_stream() << "[fission_decomp] Step 1: Clearing global scope for 0x" 
              << std::hex << address << std::dec << std::endl;
    
    // Clear global scope
    ghidra::Scope* global_scope = arch->symboltab->getGlobalScope();
    global_scope->clear();
    
    // FISSION IMPROVEMENT: Re-register cached data section symbols after clear
    if (state.data_symbols_scanned && !state.data_section_symbols.empty()) {
        fission::utils::log_stream() << "[fission_decomp] Re-registering " << state.data_section_symbols.size() 
                  << " cached data section symbols..." << std::endl;
        
        std::vector<loaders::DataSymbol> cached_symbols;
        cached_symbols.reserve(state.data_section_symbols.size());
        for (const auto& pair : state.data_section_symbols) {
            const auto& info = pair.second;
            loaders::DataSymbol sym;
            sym.address = pair.first;
            sym.name = info.name;
            sym.size = info.size;
            sym.type_meta = info.type_meta;
            cached_symbols.push_back(std::move(sym));
        }

        int registered = core::registerDataSymbolsInGlobalScope(arch, cached_symbols);
        
        fission::utils::log_stream() << "[fission_decomp] Re-registered " << registered 
                  << " data section symbols" << std::endl;
    }
    
    fission::utils::log_stream() << "[fission_decomp] Step 2: Adding function at 0x" 
              << std::hex << address << std::dec << std::endl;
    
    // Add function
    ghidra::Address func_addr(arch->getDefaultCodeSpace(), address);
    ghidra::Funcdata* fd = global_scope->findFunction(func_addr);
    if (fd == nullptr) {
        fd = global_scope->addFunction(func_addr, "func")->getFunction();
    }

    // Step 2a: Follow control flow to discover instructions (CRITICAL)
    // Without this, the decompiler sees an empty function and generates a recursive stub.
    //
    // Dynamic upper bound: use the distance from func_addr to the closest
    // *later* known function address (IAT or FID-resolved) as a tight bound,
    // capped at k_follow_flow_limit (64 KB).  This prevents over-scan into
    // adjacent functions while still covering unusually large functions.
    try {
        fission::utils::log_stream() << "[fission_decomp] Step 2a: Following control flow..." << std::endl;
        fd->clear();

        // Find the nearest known function after `address`.
        uint64_t next_func_addr = address + fission::decompiler::k_follow_flow_limit;
        // Check IAT symbols (external imports — usually not code, but a bound)
        for (const auto& kv : state.iat_symbols) {
            if (kv.first > address && kv.first < next_func_addr)
                next_func_addr = kv.first;
        }
        // Check FID-resolved internal functions
        for (const auto& kv : state.fid_function_names) {
            if (kv.first > address && kv.first < next_func_addr)
                next_func_addr = kv.first;
        }
        size_t follow_limit = static_cast<size_t>(next_func_addr - address);
        // Clamp to [0x200 .. k_follow_flow_limit] for safety.
        follow_limit = std::max<size_t>(follow_limit, 0x200);
        follow_limit = std::min<size_t>(follow_limit, fission::decompiler::k_follow_flow_limit);

        fission::utils::log_stream() << "[fission_decomp] followFlow limit: 0x"
                  << std::hex << follow_limit << std::dec << " bytes" << std::endl;

        ghidra::Address end_addr = func_addr + static_cast<ghidra::uintb>(follow_limit);
        fd->followFlow(func_addr, end_addr);
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[fission_decomp] WARNING: flow analysis failed: " << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[fission_decomp] WARNING: flow analysis failed (unknown error)" << std::endl;
    }

    // Step 2b: Detect and apply calling convention for this function
    {
        CallingConvDetector detector(arch);
        // Provide binary format hint so the detector can adjust heuristics
        // and choose the correct fallback when detection is ambiguous.
        detector.set_format_hint(compiler_id);
        auto conv = detector.detect(fd);
        if (conv == CallingConvDetector::CONV_UNKNOWN) {
            if (is_64bit) {
                // Use compiler_id to pick the correct 64-bit ABI.
                // PE/windows -> MS x64 (__fastcall), ELF/Mach-O -> SYSV x64.
                conv = (compiler_id == "windows")
                    ? CallingConvDetector::CONV_MS_X64
                    : CallingConvDetector::CONV_SYSV_X64;
            } else {
                conv = CallingConvDetector::CONV_CDECL;
            }
        }
        detector.apply(fd, conv);
    }
    
    fission::utils::log_stream() << "[fission_decomp] Step 3: Resetting actions" << std::endl;
    arch->clearAnalysis(fd);
    arch->allacts.getCurrent()->reset(*fd);

    // P1-A: Seed Ghidra's type recommendation system BEFORE action->perform()
    // so that ActionInferTypes picks up API-derived types in its own loop.
    {
        fission::analysis::TypePropagator seeder(arch, &state.struct_registry);
        seeder.set_compiler_id(compiler_id);
        seeder.seed_before_action(fd);
    }

    // Step 3b: Enforce GDT prototypes
    {
        PrototypeEnforcer proto_enforcer;
        proto_enforcer.enforce_iat_prototypes(arch, state.iat_symbols);
        auto it = state.iat_symbols.find(address);
        if (it != state.iat_symbols.end()) {
            proto_enforcer.enforce_single_prototype(arch, address, it->second);
        }
    }
    
    // Step 4: Primary Decompilation
    {
        StepTimer timer("Step 4: Decompilation");
        fission::utils::log_stream() << "[fission_decomp] Step 4: Performing decompilation..." << std::endl;
        arch->allacts.getCurrent()->perform(*fd);
    }

    // Step 4b: Advanced structure recovery / type analysis
    // (shared implementation with FFI path via run_analysis_passes)
    BatchAnalysisContext batch_ctx;
    batch_ctx.arch           = arch;
    batch_ctx.type_registry  = &state.type_registry;
    batch_ctx.symbols        = &state.iat_symbols;
    batch_ctx.struct_registry = &state.struct_registry;
    batch_ctx.data_start     = state.data_section_start;
    batch_ctx.data_end       = state.data_section_end;
    // C-1 + Fix: Populate executable_ranges from parsed section table so that
    // callgraph pending-reanalysis is not silently skipped for valid addresses.
    for (const auto& r : state.executable_ranges) {
        batch_ctx.executable_ranges.push_back(r);
    }

    AnalysisArtifacts analysis_artifacts;
    {
        StepTimer timer("Step 4b: Analysis passes");
        analysis_artifacts = run_analysis_passes(batch_ctx, fd,
            arch->allacts.getCurrent(), MAX_FUNCTION_SIZE);
    }

    // Step 4c: Emulation-Assisted Analysis
    EmulationAnalyzer emu_analyzer;
    bool emu_tags_found = emu_analyzer.analyze(fd);
    if (emu_tags_found) {
        fission::utils::log_stream() << "[fission_decomp] Step 4c: Emulation meta-tags added!" << std::endl;
    }

    // Step 4d: Enum inference — detect comparison/jump-table enums and apply types
    {
        StepTimer timer("Step 4d: Enum inference");
        int enums_created = fission::analysis::EnumInferrer::infer_and_apply(fd);
        if (enums_created > 0)
            fission::utils::log_stream() << "[fission_decomp] Step 4d: " << enums_created << " enum(s) inferred" << std::endl;
    }

    fission::utils::log_stream() << "[fission_decomp] Step 5: Generating output" << std::endl;
    std::ostringstream c_stream;
    arch->print->setOutputStream(&c_stream);
    arch->print->docFunction(fd);

    fission::utils::log_stream() << "[fission_decomp] Step 6: Post-processing pipeline" << std::endl;
    std::string c_code = c_stream.str();

    // Inject inferred struct definitions from analysis pass
    if (!analysis_artifacts.inferred_struct_definitions.empty()) {
        c_code = analysis_artifacts.inferred_struct_definitions + "\n" + c_code;
        c_code = TypePropagator::apply_struct_types(c_code, fd, analysis_artifacts.captured_structs);
    }
    // Inject inferred union definitions (Phase 2)
    if (!analysis_artifacts.inferred_union_definitions.empty()) {
        c_code = analysis_artifacts.inferred_union_definitions + "\n" + c_code;
    }

    c_code = post_process_iat_calls(c_code, state.iat_symbols);
    c_code = smart_constant_replace(c_code);
    c_code = post_process_constants(c_code, state.enum_values);
    
    // Step 6d: GUID substitution — load on first use, then cache in state
    if (state.guid_map.empty()) {
        // B-1: Single authoritative source: fission::config::get_guid_files()
        // Eliminates the previous inline list that diverged from PathConfig.cc.
        for (const auto& path : fission::config::get_guid_files()) {
            if (file_exists(path)) {
                fission::utils::log_stream() << "[fission_decomp] Loading GUIDs from: " << path << std::endl;
                std::string content = read_file_content(path);
                if (!content.empty()) {
                    std::map<std::string, std::string> loaded = load_guids_to_map(content);
                    state.guid_map.insert(loaded.begin(), loaded.end());
                }
            }
        }
        if (!state.guid_map.empty()) {
            fission::utils::log_stream() << "[fission_decomp] Loaded " << state.guid_map.size()
                     << " GUIDs/IIDs." << std::endl;
        }
    }
    c_code = substitute_guids(c_code, state.guid_map);
    
    // Apply remaining post-processors
    c_code = recover_unicode_strings(c_code);
    c_code = replace_interlocked_patterns(c_code);
    c_code = standardize_variable_names(c_code);
    c_code = replace_xunknown_types(c_code);
    c_code = cleanup_seh_boilerplate(c_code);
    c_code = demangle_cpp_names(c_code);
    c_code = normalize_cpp_virtual_calls(
        c_code,
        state.vtable_virtual_names,
        state.vcall_slot_name_hints,
        state.vcall_slot_target_hints
    );
    c_code = apply_function_signatures(c_code);
    c_code = normalize_mingw_printf_args(c_code);
    c_code = normalize_msvc_crt_printf(c_code);
    c_code = improve_internal_function_names(c_code);
    c_code = annotate_structure_offsets(c_code);
    c_code = annotate_bitfield_extractions(c_code);  // Phase 4: bitfield annotations
    c_code = apply_fid_names(c_code, state.fid_function_names);
    c_code = PostProcessor::process(c_code);
    
    fission::utils::log_stream() << "[fission_decomp] Step 7: Done!" << std::endl;
    return "{\"status\":\"ok\",\"code\":\"" + json_escape(c_code) + "\"}";
}

} // namespace decompiler
} // namespace fission
