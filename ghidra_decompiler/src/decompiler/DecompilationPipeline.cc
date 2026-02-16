#include "fission/decompiler/DecompilationPipeline.h"
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

// Global struct registry - define here to avoid linker issues
// This will be the primary definition for both executable and shared library
std::map<uint64_t, std::map<int, std::string>> global_struct_registry;

using namespace fission::utils;
using namespace fission::loader;
using namespace fission::types;
using namespace fission::processing;
using namespace fission::analysis;

namespace fission {
namespace decompiler {

// Budget guards for structure recovery
static constexpr size_t MAX_FUNCTION_SIZE = 10000;  // 10KB code limit
static constexpr int MAX_PTRSUB_OPS = 100;           // Limit analyzed operations

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
    const std::map<uint64_t, std::string>& known_strings
) {
    std::vector<std::string> refs;
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
    std::string req_sleigh_id = extract_json_string(input, "sleigh_id");
    std::string req_compiler_id = extract_json_string(input, "compiler_id");
    
    // Phase 1: Binary Format Detection
    BinaryInfo bin_info;
    if (!req_sleigh_id.empty()) {
        fission::utils::log_stream() << "[fission_decomp] Using provided architecture: " << req_sleigh_id << std::endl;
        bin_info.sleigh_id = req_sleigh_id;
        bin_info.compiler_id = req_compiler_id.empty() ? "default" : req_compiler_id;
        
        // Infer format and bitness from sleigh_id if possible
        if (req_sleigh_id.find(":64:") != std::string::npos) bin_info.is_64bit = true;
        if (req_sleigh_id.find("x86") != std::string::npos) bin_info.arch = ArchType::X86_64; // simplified
        
        // Map compiler_id to format for internal logic
        if (bin_info.compiler_id == "windows") bin_info.format = BinaryFormat::PE;
        else if (bin_info.compiler_id == "gcc") bin_info.format = BinaryFormat::ELF;
        else if (bin_info.compiler_id == "clang") bin_info.format = BinaryFormat::MACHO;
    } else {
        fission::utils::log_stream() << "[fission_decomp] Debug: Detecting Binary Format..." << std::endl;
        bin_info = BinaryDetector::detect(bin_bytes.data(), bin_bytes.size());
    }
    
    bool is_pe = (bin_info.format == BinaryFormat::PE);
    bool is_elf = (bin_info.format == BinaryFormat::ELF);
    bool is_macho = (bin_info.format == BinaryFormat::MACHO);
    std::string compiler_id = bin_info.compiler_id.empty() ? "default" : bin_info.compiler_id;
    
    if (bin_info.format != BinaryFormat::UNKNOWN || !bin_info.sleigh_id.empty()) {
        fission::utils::log_stream() << "[fission_decomp] Binary Info: " 
                  << (is_pe ? "PE" : (is_elf ? "ELF" : (is_macho ? "Mach-O" : "Unknown")))
                  << " " << (bin_info.is_64bit ? "64-bit" : "32-bit") 
                  << " Arch=" << bin_info.sleigh_id
                  << " Compiler=" << compiler_id << std::endl;
    } else {
        fission::utils::log_stream() << "[fission_decomp] Warning: Unknown binary format, assuming PE x64" << std::endl;
        bin_info.format = BinaryFormat::PE;
        bin_info.is_64bit = true;
        bin_info.sleigh_id = "x86:LE:64:default";
        compiler_id = "windows";
    }
    
    // Phase 2: Setup 64-bit Architecture
    if (!state.arch_64bit_ready) {
        // Safety: delete if exists
        if (state.loader_64bit) delete state.loader_64bit;
        if (state.arch_64bit) delete state.arch_64bit;
        
        // Phase 3: RTTI Recovery
        std::map<uint64_t, std::string> recovered_classes;
        if (is_pe || is_elf || is_macho) {
            fission::utils::log_stream() << "[fission_decomp] Debug: Running RTTI Recovery..." << std::endl;
            recovered_classes = RttiAnalyzer::recover_class_names(bin_bytes, image_base, bin_info.is_64bit);
            if (!recovered_classes.empty()) {
                fission::utils::log_stream() << "[fission_decomp] Recovered " << recovered_classes.size() 
                         << " class names via RTTI." << std::endl;
            }
        }
        
        // Phase 4: VTable Analysis
        VTableAnalyzer vtable_analyzer;
        vtable_analyzer.scan_vtables(bin_bytes.data(), bin_bytes.size(), image_base, bin_info.is_64bit);
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
                std::string display_name = vtable_analyzer.get_virtual_call_name(vt.address, slot_offset, ptr_size);
                uint64_t resolved_target = vtable_analyzer.resolve_virtual_call(vt.address, slot_offset, ptr_size);

                if (resolved_target != 0) {
                    if (!ambiguous_slot_targets.count(slot_offset)) {
                        auto it_slot_target = state.vcall_slot_target_hints.find(slot_offset);
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
        uint64_t data_start = image_base + (bin_bytes.size() / 2);  // Rough estimate
        uint64_t data_end = image_base + bin_bytes.size();
        global_analyzer.set_data_section(data_start, data_end);
        state.data_section_start = data_start;
        state.data_section_end = data_end;
        
        // Phase 6: Pattern Matching
        fission::utils::log_stream() << "[fission_decomp] Debug: Loading Patterns..." << std::endl;
        auto patterns = PatternLoader::load_standard_patterns();
        
        fission::utils::log_stream() << "[fission_decomp] Debug: Running Pattern Matching..." << std::endl;
        auto matches = PatternLoader::match_functions(bin_bytes, image_base, patterns);
        if (!matches.empty()) {
            fission::utils::log_stream() << "[fission_decomp] Identified " << matches.size() 
                     << " standard library functions via patterns." << std::endl;
            state.iat_symbols.insert(matches.begin(), matches.end());
        }
        
        // Phase 7: String Scanning (pre-scan for matcher + later substitutions)
        fission::utils::log_stream() << "[fission_decomp] Debug: String Scanning..." << std::endl;
        auto ascii_strings = StringScanner::scan_ascii_strings(bin_bytes, image_base);
        auto unicode_strings = StringScanner::scan_unicode_strings(bin_bytes, image_base);
        std::map<uint64_t, std::string> scanned_strings = ascii_strings;
        scanned_strings.insert(unicode_strings.begin(), unicode_strings.end());

        fission::utils::log_stream() << "[fission_decomp] Scanned " << ascii_strings.size()
                  << " ASCII and " << unicode_strings.size() << " Unicode strings." << std::endl;

        // Phase 8: FID Analysis + Internal matcher
        std::string fid_filename;
        if (is_pe) {
            fid_filename = TypePropagator::get_fid_filename(bin_info.is_64bit, compiler_id);
        }
        
        if (!fid_filename.empty()) {
            // Use centralized path finder
            std::string fid_path = find_fid_file(fid_filename);
            
            if (!fid_path.empty()) {
                if (file_exists(fid_path)) {

                    fission::utils::log_stream() << "[fission_decomp] Loading FID database: " << fid_path << std::endl;
                    FidDatabase fid_db;
                    if (fid_db.load(fid_path)) {
                        int matches_found = 0;
                        int internal_matches_found = 0;
                        int internal_string_matches_found = 0;
                        InternalMatcher internal_matcher;
                        // Scan for function prologues (enhanced pattern matching)
                        size_t step = 1;
                        for (size_t i = 0; i < bin_bytes.size() - 32; i += step) {
                            bool possible_start = false;
                            if (bin_info.is_64bit) {
                                // x64 Prologues (comprehensive patterns)
                                uint8_t b0 = bin_bytes[i];
                                uint8_t b1 = (i+1 < bin_bytes.size()) ? bin_bytes[i+1] : 0;
                                uint8_t b2 = (i+2 < bin_bytes.size()) ? bin_bytes[i+2] : 0;
                                uint8_t b3 = (i+3 < bin_bytes.size()) ? bin_bytes[i+3] : 0;
                                
                                // Push register prologues (40 5x, 41 5x)
                                if (b0 == 0x40 && (b1 >= 0x50 && b1 <= 0x57)) 
                                    possible_start = true;
                                if (b0 == 0x41 && (b1 >= 0x50 && b1 <= 0x57))
                                    possible_start = true;
                                
                                // Stack frame setup: sub rsp, imm (48 83 EC xx, 48 81 EC xx xx xx xx)
                                if (b0 == 0x48 && b1 == 0x83 && b2 == 0xEC) 
                                    possible_start = true;
                                if (b0 == 0x48 && b1 == 0x81 && b2 == 0xEC)
                                    possible_start = true;
                                
                                // Frame pointer setup: mov [rsp+x], reg (48 89 xx, 4C 89 xx)
                                if (b0 == 0x48 && b1 == 0x89) 
                                    possible_start = true;
                                if (b0 == 0x4C && b1 == 0x89)
                                    possible_start = true;
                                
                                // Leaf functions: mov eax, imm (B8 xx xx xx xx)
                                if (b0 >= 0xB8 && b0 <= 0xBF)
                                    possible_start = true;
                                
                                // Simple return functions: xor eax, eax (31 C0 or 33 C0)
                                if ((b0 == 0x31 || b0 == 0x33) && b1 == 0xC0)
                                    possible_start = true;
                                
                                // Test/cmp patterns at function start
                                if (b0 == 0x48 && b1 == 0x85 && (b2 >= 0xC0 && b2 <= 0xFF))  // test reg, reg
                                    possible_start = true;
                                
                                // CRT/MinGW patterns: push rbp; mov rbp, rsp (55 48 89 E5)
                                if (b0 == 0x55 && b1 == 0x48 && b2 == 0x89 && b3 == 0xE5)
                                    possible_start = true;
                                    
                            } else {
                                // x86 Prologues (comprehensive patterns)
                                uint8_t b0 = bin_bytes[i];
                                uint8_t b1 = (i+1 < bin_bytes.size()) ? bin_bytes[i+1] : 0;
                                uint8_t b2 = (i+2 < bin_bytes.size()) ? bin_bytes[i+2] : 0;
                                uint8_t b3 = (i+3 < bin_bytes.size()) ? bin_bytes[i+3] : 0;
                                
                                // Classic frame setup: push ebp; mov ebp, esp (55 8B EC or 55 89 E5)
                                if (b0 == 0x55 && b1 == 0x8B && b2 == 0xEC) 
                                    possible_start = true;
                                if (b0 == 0x55 && b1 == 0x89 && b2 == 0xE5)
                                    possible_start = true;
                                
                                // Stack allocation: sub esp, imm (83 EC xx, 81 EC xx xx xx xx)
                                if (b0 == 0x83 && b1 == 0xEC) 
                                    possible_start = true;
                                if (b0 == 0x81 && b1 == 0xEC)
                                    possible_start = true;
                                
                                // Push multiple registers: push edi/esi/ebx (57, 56, 53)
                                if (b0 >= 0x50 && b0 <= 0x57)
                                    possible_start = true;
                                
                                // Leaf functions: mov eax, imm (B8 xx xx xx xx)
                                if (b0 >= 0xB8 && b0 <= 0xBF)
                                    possible_start = true;
                                
                                // Simple return: xor eax, eax (31 C0 or 33 C0)
                                if ((b0 == 0x31 || b0 == 0x33) && b1 == 0xC0)
                                    possible_start = true;
                                
                                // __cdecl with stack check: mov eax, [esp+4] (8B 44 24 04)
                                if (b0 == 0x8B && b1 == 0x44 && b2 == 0x24)
                                    possible_start = true;
                            }
                            
                            if (possible_start) {
                                uint64_t addr = image_base + i;

                                // Internal signature matching (prologue-based)
                                if (state.iat_symbols.find(addr) == state.iat_symbols.end()) {
                                    int prologue_len = static_cast<int>(std::min<size_t>(16, bin_bytes.size() - i));
                                    std::string internal_name = internal_matcher.match_by_prologue(
                                        addr,
                                        &bin_bytes[i],
                                        prologue_len
                                    );
                                    if (!internal_name.empty()) {
                                        state.iat_symbols[addr] = internal_name;
                                        internal_matches_found++;
                                        continue;
                                    }

                                    // Internal signature matching (referenced strings)
                                    std::vector<std::string> local_refs = collect_referenced_strings_near(
                                        bin_bytes,
                                        i,
                                        image_base,
                                        scanned_strings
                                    );
                                    if (!local_refs.empty()) {
                                        std::string by_strings = internal_matcher.match_by_strings(addr, local_refs);
                                        if (!by_strings.empty()) {
                                            state.iat_symbols[addr] = by_strings;
                                            internal_string_matches_found++;
                                            continue;
                                        }
                                    }
                                }

                                size_t len = std::min((size_t)64, bin_bytes.size() - i);
                                uint64_t full_hash = FidHasher::calculate_full_hash(&bin_bytes[i], len);
                                uint64_t specific_hash = FidHasher::calculate_specific_hash(&bin_bytes[i], len);
                                
                                // Use combined hash matching (more accurate)
                                auto names = fid_db.lookup_by_hashes(full_hash, specific_hash);
                                
                                if (!names.empty()) {
                                    std::string name = names[0];
                                    
                                    // Skip if already identified or common symbol
                                    if (state.iat_symbols.find(addr) == state.iat_symbols.end() && 
                                        !fid_db.is_common_symbol(name)) {
                                        state.iat_symbols[addr] = name;
                                        matches_found++;
                                        
                                        if (matches_found <= 5) {
                                            fission::utils::log_stream() << "[FID] Matched @ 0x" << std::hex << addr 
                                                     << " -> " << name << std::dec << std::endl;
                                        }
                                    }
                                }
                            }
                        }
                        fission::utils::log_stream() << "[fission_decomp] FID Analysis: Identified " 
                                 << matches_found << " functions." << std::endl;
                        if (internal_matches_found > 0) {
                            fission::utils::log_stream() << "[fission_decomp] InternalMatcher: Identified "
                                      << internal_matches_found << " functions." << std::endl;
                        }
                        if (internal_string_matches_found > 0) {
                            fission::utils::log_stream() << "[fission_decomp] InternalMatcher(strings): Identified "
                                      << internal_string_matches_found << " functions." << std::endl;
                        }
                    }
                }
            }
        }
        
        // Feed discovered strings into constant substitution maps
        state.enum_values.insert(ascii_strings.begin(), ascii_strings.end());
        state.enum_values.insert(unicode_strings.begin(), unicode_strings.end());
        
        // Phase 9: Setup Architecture
        fission::utils::log_stream() << "[fission_decomp] Debug: Creating Loader and Arch..." << std::endl;
        state.setup_architecture(true, bin_bytes, image_base, compiler_id);
        
        // FISSION IMPROVEMENT: Phase 9.5: Scan data sections for floating-point constants
        fission::utils::log_stream() << "[fission_decomp] Debug: Scanning data sections for constants..." << std::endl;
        
        // Simple PE section parsing (inline)
        struct SimplePeSection {
            std::string name;
            uint64_t va_addr;
            uint32_t file_offset;
            uint32_t file_size;
        };
        
        std::vector<SimplePeSection> data_sections;
        
        if (is_pe && bin_bytes.size() > 0x200) {
            // Quick PE section extraction
            // DOS header check
            if (bin_bytes[0] == 'M' && bin_bytes[1] == 'Z') {
                uint32_t pe_offset = *reinterpret_cast<const uint32_t*>(&bin_bytes[0x3C]);
                if (pe_offset + 0x200 < bin_bytes.size()) {
                    // PE header check
                    if (bin_bytes[pe_offset] == 'P' && bin_bytes[pe_offset+1] == 'E') {
                        // Number of sections
                        uint16_t num_sections = *reinterpret_cast<const uint16_t*>(&bin_bytes[pe_offset + 6]);
                        uint16_t optional_header_size = *reinterpret_cast<const uint16_t*>(&bin_bytes[pe_offset + 20]);
                        
                        // Section table starts after optional header
                        uint32_t section_table_offset = pe_offset + 24 + optional_header_size;
                        
                        fission::utils::log_stream() << "[fission_decomp] PE has " << num_sections << " sections" << std::endl;
                        
                        for (int i = 0; i < num_sections && i < 64; i++) {
                            uint32_t section_offset = section_table_offset + i * 40;
                            if (section_offset + 40 > bin_bytes.size()) break;
                            
                            // Section name (8 bytes)
                            char name_buf[9] = {0};
                            memcpy(name_buf, &bin_bytes[section_offset], 8);
                            std::string section_name(name_buf);
                            
                            // Read section header fields
                            uint32_t virtual_size = *reinterpret_cast<const uint32_t*>(&bin_bytes[section_offset + 8]);
                            uint32_t virtual_addr = *reinterpret_cast<const uint32_t*>(&bin_bytes[section_offset + 12]);
                            uint32_t raw_size = *reinterpret_cast<const uint32_t*>(&bin_bytes[section_offset + 16]);
                            uint32_t raw_offset = *reinterpret_cast<const uint32_t*>(&bin_bytes[section_offset + 20]);
                            
                            // Check if it's a data section
                            if (section_name.find(".rdata") != std::string::npos ||
                                section_name.find(".data") != std::string::npos) {
                                SimplePeSection sec;
                                sec.name = section_name;
                                sec.va_addr = image_base + virtual_addr;
                                sec.file_offset = raw_offset;
                                sec.file_size = raw_size;
                                data_sections.push_back(sec);
                                
                                fission::utils::log_stream() << "[fission_decomp] Found data section: " << section_name 
                                          << " VA=0x" << std::hex << sec.va_addr 
                                          << " offset=0x" << raw_offset 
                                          << " size=" << std::dec << raw_size << std::endl;
                            }
                        }
                    }
                }
            }
        }
        
        loaders::DataSectionScanner data_scanner;
        int total_data_symbols = 0;
        
        for (const auto& section : data_sections) {
            fission::utils::log_stream() << "[fission_decomp] Scanning section: " << section.name 
                      << " at 0x" << std::hex << section.va_addr 
                      << " size=" << std::dec << section.file_size << std::endl;
            
            // Check if we have the data
            size_t start_idx = section.file_offset;
            size_t end_idx = start_idx + section.file_size;
            
            if (end_idx > bin_bytes.size()) {
                fission::utils::log_stream() << "[fission_decomp] Warning: section extends beyond binary data" << std::endl;
                continue;
            }
            
            // Get pointer to section data
            const uint8_t* section_data = bin_bytes.data() + start_idx;
            
            // Scan for symbols
            std::vector<loaders::DataSymbol> symbols = data_scanner.scanDataSection(
                section_data,
                section.va_addr,
                section.file_size
            );
            
            total_data_symbols += core::registerDataSymbolsInGlobalScope(
                state.arch_64bit,
                symbols,
                [&](const loaders::DataSymbol& sym) {
                    core::DecompilerContext::DataSymbolInfo info;
                    info.name = sym.name;
                    info.size = sym.size;
                    info.type_meta = sym.type_meta;
                    state.data_section_symbols[sym.address] = info;
                }
            );
        }
        
        state.data_symbols_scanned = true;
        fission::utils::log_stream() << "[fission_decomp] Registered " << total_data_symbols 
                  << " data section symbols (cached for future use)" << std::endl;
        
    } else {
        state.loader_64bit->updateData(bin_bytes, image_base);
        state.arch_64bit->symboltab->getGlobalScope()->clear();
    }
    
    // Phase 10: Inject IAT symbols for 64-bit
    auto iat_symbols = extract_iat_symbols(input);
    fission::utils::log_stream() << "[fission_decomp] Parsed " << iat_symbols.size() 
              << " IAT symbols from JSON" << std::endl;
    state.arch_64bit->injectIatSymbols(iat_symbols);
    state.iat_symbols = iat_symbols;
    
    // Phase 11: Setup 32-bit Architecture
    if (!state.arch_32bit_ready) {
        state.setup_architecture(false, bin_bytes, image_base, compiler_id);
    } else {
        state.loader_32bit->updateData(bin_bytes, image_base);
        state.arch_32bit->symboltab->getGlobalScope()->clear();
    }
    
    // Inject IAT symbols for 32-bit
    state.arch_32bit->injectIatSymbols(iat_symbols);
    
    // Phase 12: Load FID databases
    {
        // Use centralized path configuration
        std::vector<std::string> fid_candidates = ::fission::config::get_all_fid_paths(bin_info.is_64bit);
        
        static FidDatabase fid_db;
        static FunctionMatcher func_matcher;
        static bool fid_initialized = false;
        
        if (!fid_initialized) {
            for (const auto& path : fid_candidates) {
                if (fid_db.load(path)) {
                    fission::utils::log_stream() << "[fission_decomp] Loaded FID database: " << path 
                              << " (" << fid_db.get_function_count() << " functions)" << std::endl;
                    func_matcher.set_fid_database(&fid_db);
                    break;
                }
            }
            fid_initialized = true;
        }
        
        // Load ALL available FID databases using centralized paths
        static std::vector<FidDatabase> all_fid_dbs;
        if (all_fid_dbs.empty()) {
            std::vector<std::string> all_fid_paths = ::fission::config::get_all_fid_paths(bin_info.is_64bit);
            for (const auto& path : all_fid_paths) {
                FidDatabase db;
                if (db.load(path)) {
                    all_fid_dbs.push_back(std::move(db));
                }
            }
            fission::utils::log_stream() << "[fission_decomp] Loaded " << all_fid_dbs.size() 
                     << " FID databases total" << std::endl;
        }

        // Improved function prologue detection with 2-pass Relation Validation
        fission::utils::log_stream() << "[fission_decomp] Starting FID scan on " << bin_bytes.size() << " bytes..." << std::endl;
        std::vector<uint64_t> prologues = PatternLoader::scan_function_prologues(bin_bytes, image_base);
        size_t prologue_count = prologues.size();
        fission::utils::log_stream() << "[fission_decomp] Found " << prologue_count << " prologues to evaluate." << std::endl;
        
        // Pass 1: Calculate hashes for all identified prologues
        std::map<uint64_t, uint64_t> addr_to_hash;
        for (uint64_t addr : prologues) {
            size_t offset = addr - image_base;
            if (offset + 64 >= bin_bytes.size()) continue;
            addr_to_hash[addr] = FidHasher::calculate_full_hash(&bin_bytes[offset], 64);
        }

        // Pass 2: Match and Validate relations
        size_t matched_count = 0;
        for (uint64_t addr : prologues) {
            uint64_t func_hash = addr_to_hash[addr];
            if (func_hash == 0) continue;

            // Find all candidates across all loaded databases
            std::vector<const FidFunctionRecord*> all_candidates;
            FidDatabase* best_db = nullptr;
            
            for (auto& db : all_fid_dbs) {
                std::vector<const FidFunctionRecord*> candidates = db.lookup_records_by_hash(func_hash);
                if (!candidates.empty()) {
                    all_candidates.insert(all_candidates.end(), candidates.begin(), candidates.end());
                    if (!best_db) best_db = &db; // Keep track of a DB for the validator
                }
            }

            if (all_candidates.empty()) continue;

            if (all_candidates.size() == 1) {
                // Unique match, apply immediately
                state.fid_function_names[addr] = all_candidates[0]->name;
                matched_count++;
            } else if (best_db) {
                // Multiple candidates, use RelationValidator
                RelationValidator validator(std::shared_ptr<FidDatabase>(best_db, [](FidDatabase*){})); // Quick wrap
                
                // Collect actual callee hashes (simple scan)
                std::vector<uint64_t> actual_callees;
                size_t offset = addr - image_base;
                // Limit scan to 0x100 for performance and to avoid getting stuck in loops
                for (size_t i = 0; i < 0x100 && (offset + i + 5) < bin_bytes.size(); ++i) {
                    if (bin_bytes[offset + i] == 0xE8) { // CALL rel32
                        int32_t rel = *(int32_t*)&bin_bytes[offset + i + 1];
                        uint64_t target = addr + i + 5 + rel;
                        if (addr_to_hash.count(target)) {
                            actual_callees.push_back(addr_to_hash[target]);
                        }
                    }
                    if (bin_bytes[offset + i] == 0xC3) break; // RET
                }

                auto result = validator.find_best_match(all_candidates, actual_callees);
                if (!result.name.empty()) {
                    state.fid_function_names[addr] = result.name;
                    matched_count++;
                } else if (!all_candidates.empty()) {
                     // Fallback to first candidate if validation didn't pick one but we have candidates
                     // (This is what we did before, but now we're informed by the validator)
                     state.fid_function_names[addr] = all_candidates[0]->name;
                     matched_count++;
                }
            }
        }
        
        fission::utils::log_stream() << "[fission_decomp] Scanned " << prologue_count << " function prologues" << std::endl;
        if (matched_count > 0) {
            fission::utils::log_stream() << "[fission_decomp] FID matched " << matched_count 
                     << " functions by hash" << std::endl;
        }
        
        // Use centralized common symbols path configuration
        const auto symbol_files = ::fission::config::get_common_symbol_files();
        
        for (const auto& path : symbol_files) {
            if (file_exists(path)) {
                auto symbols = SymbolLoader::load_symbols_text(path);
                for (const auto& [addr, name] : symbols) {
                    state.fid_function_names[addr] = name;
                }
                fission::utils::log_stream() << "[fission_decomp] Loaded common symbols from: " << path << std::endl;
            }
        }
    }
    
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
    state.setup_architecture(is_64bit, bytes, address, compiler_id);
    
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
    try {
        fission::utils::log_stream() << "[fission_decomp] Step 2a: Following control flow..." << std::endl;
        fd->clear();
        ghidra::Address end_addr = func_addr + 0x2000; // 8KB heuristic
        fd->followFlow(func_addr, end_addr);
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[fission_decomp] WARNING: flow analysis failed: " << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[fission_decomp] WARNING: flow analysis failed (unknown error)" << std::endl;
    }

    // Step 2b: Detect and apply calling convention for this function
    {
        CallingConvDetector detector(arch);
        auto conv = detector.detect(fd);
        if (conv == CallingConvDetector::CONV_UNKNOWN) {
            conv = is_64bit
                ? CallingConvDetector::CONV_MS_X64
                : CallingConvDetector::CONV_CDECL;
        }
        detector.apply(fd, conv);
    }
    
    fission::utils::log_stream() << "[fission_decomp] Step 3: Resetting actions" << std::endl;
    arch->clearAnalysis(fd);
    arch->allacts.getCurrent()->reset(*fd);
    
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
    
    // Step 4b: Advanced Structure Recovery (Guarded)
    std::string inferred_struct_defs;
    std::map<unsigned long long, ghidra::TypeStruct*> captured_structs;
    std::string optimized_pcode_json;
    
    size_t func_size = fd->getSize();
    if (func_size < MAX_FUNCTION_SIZE) {
        StepTimer timer("Step 4b: Structure Recovery");
        StructureAnalyzer struct_analyzer;
        bool structs_found = struct_analyzer.analyze_function_structures(fd);
        
        if (structs_found) {
            fission::utils::log_stream() << "[fission_decomp] Step 4b: New structures inferred! Re-running decompilation..." 
                     << std::endl;
            try {
                inferred_struct_defs = struct_analyzer.generate_struct_definitions();
                captured_structs = struct_analyzer.get_inferred_structs();
                
                fd->clear();
                arch->allacts.getCurrent()->reset(*fd);
                arch->allacts.getCurrent()->perform(*fd);
                
                // Register inferred types
                fission::utils::log_stream() << "[fission_decomp] Registering inferred types for 0x" 
                         << std::hex << fd->getAddress().getOffset() << std::dec << std::endl;
                const ghidra::FuncProto& proto = fd->getFuncProto();
                int num = proto.numParams();
                for(int i=0; i<num; ++i) {
                    ghidra::ProtoParameter* param = proto.getParam(i);
                    uint64_t off = param->getAddress().getOffset();
                    if (captured_structs.count(off)) {
                        std::string sname = captured_structs[off]->getName();
                        global_struct_registry[fd->getAddress().getOffset()][i] = sname;
                    }
                }
            } catch (const ghidra::LowlevelError& e) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b ERROR: " << e.explain << std::endl;
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b EXCEPTION: " << e.what() << std::endl;
            }
        }
        
        // Step 4b-2: Reverse Type Propagation
        StepTimer timer_rev("Step 4b-2: Reverse Propagation");
        // Use TypePropagator with struct registry
        TypePropagator type_propagator(arch, &global_struct_registry);
        type_propagator.clear();
        bool struct_changed = type_propagator.propagate_struct_types(fd);
        if (struct_changed) {
            fission::utils::log_stream() << "[fission_decomp] Step 4b-2: Reverse propagation applied! Re-running..."
                     << std::endl;
            fd->clear();
            arch->allacts.getCurrent()->reset(*fd);
            arch->allacts.getCurrent()->perform(*fd);
            type_propagator.clear();
        }

        int types_inferred = type_propagator.propagate(fd);
        bool struct_changed_after = type_propagator.propagate_struct_types(fd);
        if (types_inferred > 0 || struct_changed_after) {
            fission::utils::log_stream() << "[fission_decomp] Step 4b-2: Type propagation complete, re-running..."
                     << std::endl;
            fd->clear();
            arch->allacts.getCurrent()->reset(*fd);
            arch->allacts.getCurrent()->perform(*fd);
        }

        // Step 4b-3: Global data structure recovery
        bool rerun_for_struct_symbols = false;
        {
            GlobalDataAnalyzer global_analyzer;
            if (state.data_section_start < state.data_section_end) {
                global_analyzer.set_data_section(state.data_section_start, state.data_section_end);
            }
            global_analyzer.analyze_function(fd);
            global_analyzer.infer_structures();
            int created = global_analyzer.create_types(arch->types, arch->types->getSizeOfPointer());
            if (created > 0) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-3: Global data structures created: "
                          << created << std::endl;
            }

            ghidra::Scope* global_scope_gd = arch->symboltab->getGlobalScope();
            ghidra::AddrSpace* data_space_gd = arch->getDefaultDataSpace();
            if (global_scope_gd && data_space_gd) {
                for (const auto& gs : global_analyzer.get_structures()) {
                    if (gs.name.empty()) {
                        continue;
                    }
                    ghidra::Datatype* dt = arch->types->findByName(gs.name);
                    if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) {
                        continue;
                    }
                    ghidra::Address addr_gd(data_space_gd, gs.address);
                    if (ghidra::SymbolEntry* entry = global_scope_gd->findAddr(addr_gd, fd->getAddress())) {
                        ghidra::Symbol* sym = entry->getSymbol();
                        if (sym) {
                            try {
                                global_scope_gd->retypeSymbol(sym, dt);
                                global_scope_gd->setAttribute(sym, ghidra::Varnode::typelock);
                                rerun_for_struct_symbols = true;
                            } catch (const ghidra::RecovError&) {
                                // ignore retype failures
                            }
                        }
                        continue;
                    }
                    if (global_scope_gd->addSymbol(gs.name, dt, addr_gd, fd->getAddress())) {
                        rerun_for_struct_symbols = true;
                    }
                }
            }
        }

        if (rerun_for_struct_symbols) {
            fission::utils::log_stream() << "[fission_decomp] Step 4b-3: Struct symbols applied, re-running..."
                      << std::endl;
            fd->clear();
            arch->allacts.getCurrent()->reset(*fd);
            arch->allacts.getCurrent()->perform(*fd);
        }

        // Step 4b-4: Call graph analysis + cross-function type registry
        {
            using namespace fission::types;
            FunctionSignature sig;
            uint64_t func_addr_cg = fd->getAddress().getOffset();
            sig.address = func_addr_cg;
            sig.return_type = nullptr;
            const ghidra::FuncProto& proto_cg = fd->getFuncProto();
            ghidra::ProtoParameter* ret_cg = proto_cg.getOutput();
            if (ret_cg && ret_cg->getType()) {
                ghidra::Datatype* rt = ret_cg->getType();
                if (rt->getMetatype() == ghidra::TYPE_STRUCT) {
                    sig.return_type = dynamic_cast<ghidra::TypeStruct*>(rt);
                }
            }
            int num_pcg = proto_cg.numParams();
            for (int i = 0; i < num_pcg; ++i) {
                ghidra::ProtoParameter* param = proto_cg.getParam(i);
                if (!param || !param->getType()) continue;
                ParamTypeInfo pinfo;
                pinfo.param_index = i;
                pinfo.struct_type = nullptr;
                ghidra::Datatype* ptype = param->getType();
                pinfo.type_name = ptype->getName();
                pinfo.is_pointer = (ptype->getMetatype() == ghidra::TYPE_PTR);
                if (ptype->getMetatype() == ghidra::TYPE_STRUCT) {
                    pinfo.struct_type = dynamic_cast<ghidra::TypeStruct*>(ptype);
                } else if (pinfo.is_pointer) {
                    ghidra::Datatype* pointed = static_cast<ghidra::TypePointer*>(ptype)->getPtrTo();
                    if (pointed && pointed->getMetatype() == ghidra::TYPE_STRUCT) {
                        pinfo.struct_type = dynamic_cast<ghidra::TypeStruct*>(pointed);
                    }
                }
                sig.params.push_back(pinfo);
            }

            state.type_registry.register_function_types(func_addr_cg, sig);

            fission::analysis::CallGraphAnalyzer call_analyzer(&state.type_registry);
            call_analyzer.extract_calls(fd);
            int propagated = call_analyzer.propagate_types();
            if (propagated > 0) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-4: CallGraph propagated "
                          << propagated << " type hints" << std::endl;
            }

            // Consume pending queue and run bounded reanalysis for queued functions.
            auto build_sig = [&](ghidra::Funcdata* target_fd) {
                FunctionSignature target_sig;
                if (target_fd == nullptr) {
                    return target_sig;
                }
                target_sig.address = target_fd->getAddress().getOffset();
                target_sig.return_type = nullptr;

                const ghidra::FuncProto& proto_target = target_fd->getFuncProto();
                ghidra::ProtoParameter* ret_target = proto_target.getOutput();
                if (ret_target != nullptr && ret_target->getType() != nullptr) {
                    ghidra::Datatype* rt = ret_target->getType();
                    if (rt->getMetatype() == ghidra::TYPE_STRUCT) {
                        target_sig.return_type = dynamic_cast<ghidra::TypeStruct*>(rt);
                    }
                }

                int target_num = proto_target.numParams();
                for (int p = 0; p < target_num; ++p) {
                    ghidra::ProtoParameter* target_param = proto_target.getParam(p);
                    if (target_param == nullptr || target_param->getType() == nullptr) {
                        continue;
                    }
                    ParamTypeInfo target_info;
                    target_info.param_index = p;
                    target_info.struct_type = nullptr;
                    ghidra::Datatype* target_type = target_param->getType();
                    target_info.type_name = target_type->getName();
                    target_info.is_pointer = (target_type->getMetatype() == ghidra::TYPE_PTR);
                    if (target_type->getMetatype() == ghidra::TYPE_STRUCT) {
                        target_info.struct_type = dynamic_cast<ghidra::TypeStruct*>(target_type);
                    } else if (target_info.is_pointer) {
                        ghidra::Datatype* pointed = static_cast<ghidra::TypePointer*>(target_type)->getPtrTo();
                        if (pointed != nullptr && pointed->getMetatype() == ghidra::TYPE_STRUCT) {
                            target_info.struct_type = dynamic_cast<ghidra::TypeStruct*>(pointed);
                        }
                    }
                    target_sig.params.push_back(target_info);
                }

                return target_sig;
            };

            std::set<uint64_t> processed_pending;
            int reanalyzed_pending = 0;
            int rounds = 0;
            const int max_rounds = 2;

            std::vector<uint64_t> pending = state.type_registry.consume_pending_reanalysis();
            ghidra::Scope* cg_scope = arch->symboltab->getGlobalScope();
            while (!pending.empty() && rounds < max_rounds && cg_scope != nullptr) {
                ++rounds;
                for (uint64_t target_addr : pending) {
                    if (processed_pending.count(target_addr) != 0) {
                        continue;
                    }
                    processed_pending.insert(target_addr);

                    ghidra::Address target_func_addr(arch->getDefaultCodeSpace(), target_addr);
                    ghidra::Funcdata* target_fd = cg_scope->findFunction(target_func_addr);
                    if (target_fd == nullptr) {
                        ghidra::FunctionSymbol* sym = cg_scope->addFunction(target_func_addr, "sub_" + std::to_string(target_addr));
                        if (sym == nullptr) {
                            continue;
                        }
                        target_fd = sym->getFunction();
                    }
                    if (target_fd == nullptr) {
                        continue;
                    }

                    try {
                        target_fd->clear();
                        ghidra::Address target_end = target_func_addr + 0x1000;
                        target_fd->followFlow(target_func_addr, target_end);
                        arch->allacts.getCurrent()->reset(*target_fd);
                        arch->allacts.getCurrent()->perform(*target_fd);
                    } catch (...) {
                        continue;
                    }

                    FunctionSignature target_sig = build_sig(target_fd);
                    state.type_registry.register_function_types(target_sig.address, target_sig);
                    call_analyzer.extract_calls(target_fd);
                    reanalyzed_pending++;
                }

                int newly_propagated = call_analyzer.propagate_types();
                if (newly_propagated <= 0) {
                    break;
                }
                pending = state.type_registry.consume_pending_reanalysis();
            }

            if (reanalyzed_pending > 0) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-4: Reanalyzed "
                          << reanalyzed_pending << " pending functions" << std::endl;
                fd->clear();
                arch->allacts.getCurrent()->reset(*fd);
                arch->allacts.getCurrent()->perform(*fd);
            }
        }

        // Step 4b-5: Cross-function type sharing
        {
            fission::analysis::TypeSharing type_sharing(arch);
            std::vector<ghidra::Datatype*> param_types_ts;
            const ghidra::FuncProto& proto_ts = fd->getFuncProto();
            for (int i = 0; i < proto_ts.numParams(); ++i) {
                ghidra::ProtoParameter* param = proto_ts.getParam(i);
                if (param) param_types_ts.push_back(param->getType());
            }
            ghidra::ProtoParameter* ret_ts = proto_ts.getOutput();
            ghidra::Datatype* ret_type_ts = (ret_ts ? ret_ts->getType() : nullptr);
            uint64_t func_addr_ts = fd->getAddress().getOffset();
            type_sharing.register_function_types(func_addr_ts, param_types_ts, ret_type_ts);
            int shared = type_sharing.share_types();
            if (shared > 0) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-5: TypeSharing shared "
                          << shared << " types" << std::endl;
            }
        }

        // Step 4b-6: Pcode optimization bridge (attempt inject now; textual apply later)
        if (fission::decompiler::PcodeOptimizationBridge::is_enabled()) {
            try {
                optimized_pcode_json = fission::decompiler::PcodeOptimizationBridge::extract_and_optimize(fd);
                if (!optimized_pcode_json.empty()) {
                    fission::utils::log_stream() << "[fission_decomp] Step 4b-6: Pcode optimized ("
                              << optimized_pcode_json.size() << " bytes)" << std::endl;
                    if (fission::decompiler::PcodeExtractor::inject_pcode(fd, optimized_pcode_json)) {
                        fission::utils::log_stream() << "[fission_decomp] Step 4b-6: Injected optimized Pcode, re-running..."
                                  << std::endl;
                        fd->clear();
                        arch->allacts.getCurrent()->reset(*fd);
                        arch->allacts.getCurrent()->perform(*fd);
                    }
                }
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-6: Pcode optimization error: "
                          << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[fission_decomp] Step 4b-6: Pcode optimization unknown error" << std::endl;
            }
        }
    } else {
        fission::utils::log_stream() << "[fission_decomp] Step 4b: Skipped (function too large: " 
                  << func_size << " bytes > " << MAX_FUNCTION_SIZE << " limit)" << std::endl;
    }
    
    // Step 4c: Emulation-Assisted Analysis
    EmulationAnalyzer emu_analyzer;
    bool emu_tags_found = emu_analyzer.analyze(fd);
    if (emu_tags_found) {
        fission::utils::log_stream() << "[fission_decomp] Step 4c: Emulation meta-tags added!" << std::endl;
    }
    
    fission::utils::log_stream() << "[fission_decomp] Step 5: Generating output" << std::endl;
    std::ostringstream c_stream;
    arch->print->setOutputStream(&c_stream);
    arch->print->docFunction(fd);
    
    fission::utils::log_stream() << "[fission_decomp] Step 6: Post-processing pipeline" << std::endl;
    std::string c_code = c_stream.str();

    if (!optimized_pcode_json.empty()) {
        std::string transformed = fission::decompiler::PcodeExtractor::apply_optimized_pcode(fd, optimized_pcode_json);
        if (!transformed.empty()) {
            fission::utils::log_stream() << "[fission_decomp] Step 5b: Applied optimized Pcode textual transform" << std::endl;
            c_code = transformed;
        }
    }
    
    // Inject inferred struct definitions
    if (!inferred_struct_defs.empty()) {
        c_code = inferred_struct_defs + "\n" + c_code;
        c_code = TypePropagator::apply_struct_types(c_code, fd, captured_structs);
    }
    
    c_code = post_process_iat_calls(c_code, state.iat_symbols);
    c_code = smart_constant_replace(c_code);
    c_code = post_process_constants(c_code, state.enum_values);
    
    // Step 6d: GUID substitution
    if (state.guid_map.empty()) {
        std::vector<std::string> guid_files = {
            "../../utils/signatures/typeinfo/win32/msvcrt/guids.txt",
            "../utils/signatures/typeinfo/win32/msvcrt/guids.txt",
            "./utils/signatures/typeinfo/win32/msvcrt/guids.txt",
            "../../utils/signatures/typeinfo/win32/msvcrt/iids.txt",
            "../utils/signatures/typeinfo/win32/msvcrt/iids.txt",
            "./utils/signatures/typeinfo/win32/msvcrt/iids.txt"
        };
        
        for (const auto& path : guid_files) {
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
    c_code = improve_internal_function_names(c_code);
    c_code = annotate_structure_offsets(c_code);
    c_code = apply_fid_names(c_code, state.fid_function_names);
    c_code = PostProcessor::process(c_code);
    
    fission::utils::log_stream() << "[fission_decomp] Step 7: Done!" << std::endl;
    return "{\"status\":\"ok\",\"code\":\"" + json_escape(c_code) + "\"}";
}

} // namespace decompiler
} // namespace fission
