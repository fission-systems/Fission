#include "fission/decompiler/DecompilationPipeline.h"
#include "fission/decompiler/PostProcessor.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/utils/json_utils.h"
#include "fission/utils/encoding.h"
#include "fission/utils/file_utils.h"
#include "fission/utils/StepTimer.h"
#include "fission/utils/logger.h"
#include "fission/loader/BinaryDetector.h"
#include "fission/loader/PeHeader.h"
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
#include "fission/analysis/VTableAnalyzer.h"
#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/analysis/EmulationAnalyzer.h"
#include "libdecomp.hh"
#include "database.hh"
#include "type.hh"
#include <iostream>
#include <sstream>
#include <iomanip>

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

std::string DecompilationPipeline::process_request(
    core::DecompilerContext& state, 
    const std::string& input
) {
    std::cerr << "[fission_decomp] Received request: " 
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
    
    std::cerr << "[fission_decomp] load_bin: size=" << bin_bytes.size() 
              << " image_base=0x" << std::hex << image_base << std::dec << std::endl;
    
    // Debug: Print first 16 bytes
    std::cerr << "[fission_decomp] First 16 bytes: ";
    for (size_t i = 0; i < std::min((size_t)16, bin_bytes.size()); ++i) {
        std::cerr << std::hex << std::setw(2) << std::setfill('0') 
                  << (int)bin_bytes[i] << " ";
    }
    std::cerr << std::dec << std::endl;
    
    // Initialize Ghidra if needed
    if (sla_dir.empty()) {
        return "{\"status\":\"error\",\"message\":\"Missing sla_dir for load_bin\"}";
    }
    if (!state.initialize(sla_dir)) {
        return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
    }
    
    // Phase 1: Binary Format Detection
    std::cerr << "[fission_decomp] Debug: Detecting Binary Format..." << std::endl;
    BinaryInfo bin_info = BinaryDetector::detect(bin_bytes.data(), bin_bytes.size());
    
    bool is_pe = (bin_info.format == BinaryFormat::PE);
    bool is_elf = (bin_info.format == BinaryFormat::ELF);
    bool is_macho = (bin_info.format == BinaryFormat::MACHO);
    std::string compiler_id = bin_info.compiler_id.empty() ? "default" : bin_info.compiler_id;
    
    if (bin_info.format != BinaryFormat::UNKNOWN) {
        std::cerr << "[fission_decomp] Detected Binary: " 
                  << (is_pe ? "PE" : (is_elf ? "ELF" : (is_macho ? "Mach-O" : "Unknown")))
                  << " " << (bin_info.is_64bit ? "64-bit" : "32-bit") 
                  << " Arch=" << bin_info.sleigh_id
                  << " Compiler=" << compiler_id << std::endl;
    } else {
        std::cerr << "[fission_decomp] Warning: Unknown binary format, assuming PE x64" << std::endl;
        bin_info.format = BinaryFormat::PE;
        bin_info.is_64bit = true;
        bin_info.sleigh_id = "x86:LE:64:default";
        compiler_id = "windows";
    }
    
    // Backward compatibility
    PeDetectionResult pe_info;
    pe_info.is_pe = is_pe;
    pe_info.is_64bit = bin_info.is_64bit;
    pe_info.compiler_id = compiler_id;
    
    // Phase 2: Setup 64-bit Architecture
    if (!state.arch_64bit_ready) {
        // Safety: delete if exists
        if (state.loader_64bit) delete state.loader_64bit;
        if (state.arch_64bit) delete state.arch_64bit;
        
        // Phase 3: RTTI Recovery
        std::map<uint64_t, std::string> recovered_classes;
        if (is_pe || is_elf || is_macho) {
            std::cerr << "[fission_decomp] Debug: Running RTTI Recovery..." << std::endl;
            recovered_classes = RttiAnalyzer::recover_class_names(bin_bytes, image_base, bin_info.is_64bit);
            if (!recovered_classes.empty()) {
                std::cerr << "[fission_decomp] Recovered " << recovered_classes.size() 
                         << " class names via RTTI." << std::endl;
            }
        }
        
        // Phase 4: VTable Analysis
        VTableAnalyzer vtable_analyzer;
        vtable_analyzer.scan_vtables(bin_bytes.data(), bin_bytes.size(), image_base, bin_info.is_64bit);
        if (!recovered_classes.empty()) {
            vtable_analyzer.link_with_rtti(recovered_classes);
        }
        std::cerr << "[fission_decomp] VTable scan complete: " 
                  << vtable_analyzer.get_vtables().size() << " vtables found" << std::endl;
        
        // Phase 5: Global Data Analyzer
        GlobalDataAnalyzer global_analyzer;
        uint64_t data_start = image_base + (bin_bytes.size() / 2);  // Rough estimate
        uint64_t data_end = image_base + bin_bytes.size();
        global_analyzer.set_data_section(data_start, data_end);
        
        // Phase 6: Pattern Matching
        std::cerr << "[fission_decomp] Debug: Loading Patterns..." << std::endl;
        auto patterns = PatternLoader::load_standard_patterns();
        
        std::cerr << "[fission_decomp] Debug: Running Pattern Matching..." << std::endl;
        auto matches = PatternLoader::match_functions(bin_bytes, image_base, patterns);
        if (!matches.empty()) {
            std::cerr << "[fission_decomp] Identified " << matches.size() 
                     << " standard library functions via patterns." << std::endl;
            state.iat_symbols.insert(matches.begin(), matches.end());
        }
        
        // Phase 7: FID Analysis
        std::string fid_filename;
        if (is_pe) {
            fid_filename = fission::analysis::TypePropagator::get_fid_filename(pe_info.is_64bit, compiler_id);
        }
        
        if (!fid_filename.empty()) {
            std::vector<std::string> fid_paths = {
                "../../ghidra/funtionID/" + fid_filename,
                "../ghidra/funtionID/" + fid_filename,
                "./ghidra/funtionID/" + fid_filename
            };
            
            for (const auto& fid_path : fid_paths) {
                if (file_exists(fid_path)) {
                    std::cerr << "[fission_decomp] Loading FID database: " << fid_path << std::endl;
                    FidDatabase fid_db;
                    if (fid_db.load(fid_path)) {
                        int matches_found = 0;
                        // Scan for function prologues (enhanced pattern matching)
                        size_t step = 1;
                        for (size_t i = 0; i < bin_bytes.size() - 32; i += step) {
                            bool possible_start = false;
                            if (pe_info.is_64bit) {
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
                                size_t len = std::min((size_t)64, bin_bytes.size() - i);
                                uint64_t full_hash = FidHasher::calculate_full_hash(&bin_bytes[i], len);
                                uint64_t specific_hash = FidHasher::calculate_specific_hash(&bin_bytes[i], len);
                                
                                // Use combined hash matching (more accurate)
                                auto names = fid_db.lookup_by_hashes(full_hash, specific_hash);
                                
                                if (!names.empty()) {
                                    uint64_t addr = image_base + i;
                                    std::string name = names[0];
                                    
                                    // Skip if already identified or common symbol
                                    if (state.iat_symbols.find(addr) == state.iat_symbols.end() && 
                                        !fid_db.is_common_symbol(name)) {
                                        state.iat_symbols[addr] = name;
                                        matches_found++;
                                        
                                        if (matches_found <= 5) {
                                            std::cerr << "[FID] Matched @ 0x" << std::hex << addr 
                                                     << " -> " << name << std::dec << std::endl;
                                        }
                                    }
                                }
                            }
                        }
                        std::cerr << "[fission_decomp] FID Analysis: Identified " 
                                 << matches_found << " functions." << std::endl;
                    }
                    break;
                }
            }
        }
        
        // Phase 8: String Scanning
        std::cerr << "[fission_decomp] Debug: String Scanning..." << std::endl;
        auto ascii_strings = StringScanner::scan_ascii_strings(bin_bytes, image_base);
        auto unicode_strings = StringScanner::scan_unicode_strings(bin_bytes, image_base);
        std::cerr << "[fission_decomp] Scanned " << ascii_strings.size() 
                  << " ASCII and " << unicode_strings.size() << " Unicode strings." << std::endl;
        
        state.enum_values.insert(ascii_strings.begin(), ascii_strings.end());
        state.enum_values.insert(unicode_strings.begin(), unicode_strings.end());
        
        // Phase 9: Setup Architecture
        std::cerr << "[fission_decomp] Debug: Creating Loader and Arch..." << std::endl;
        state.setup_architecture(true, bin_bytes, image_base, compiler_id);
        
        // FISSION IMPROVEMENT: Phase 9.5: Scan data sections for floating-point constants
        std::cerr << "[fission_decomp] Debug: Scanning data sections for constants..." << std::endl;
        
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
                        
                        std::cerr << "[fission_decomp] PE has " << num_sections << " sections" << std::endl;
                        
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
                                
                                std::cerr << "[fission_decomp] Found data section: " << section_name 
                                          << " VA=0x" << std::hex << sec.va_addr 
                                          << " offset=0x" << raw_offset 
                                          << " size=" << std::dec << raw_size << std::endl;
                            }
                        }
                    }
                }
            }
        }
        
        fission::loaders::DataSectionScanner data_scanner;
        int total_data_symbols = 0;
        
        for (const auto& section : data_sections) {
            std::cerr << "[fission_decomp] Scanning section: " << section.name 
                      << " at 0x" << std::hex << section.va_addr 
                      << " size=" << std::dec << section.file_size << std::endl;
            
            // Check if we have the data
            size_t start_idx = section.file_offset;
            size_t end_idx = start_idx + section.file_size;
            
            if (end_idx > bin_bytes.size()) {
                std::cerr << "[fission_decomp] Warning: section extends beyond binary data" << std::endl;
                continue;
            }
            
            // Get pointer to section data
            const uint8_t* section_data = bin_bytes.data() + start_idx;
            
            // Scan for symbols
            std::vector<fission::loaders::DataSymbol> symbols = data_scanner.scanDataSection(
                section_data,
                section.va_addr,
                section.file_size
            );
            
            // Register each symbol in global scope
            ghidra::Scope* global_scope = state.arch_64bit->symboltab->getGlobalScope();
            ghidra::TypeFactory* types = state.arch_64bit->types;
            ghidra::AddrSpace* ram_space = state.arch_64bit->getDefaultDataSpace();
            
            if (!global_scope || !types || !ram_space) {
                std::cerr << "[fission_decomp] Warning: Missing required components for symbol registration" << std::endl;
                continue;
            }
            
            for (const auto& sym : symbols) {
                // Cache the symbol info
                core::DecompilerContext::DataSymbolInfo info;
                info.name = sym.name;
                info.size = sym.size;
                info.type_meta = sym.type_meta;
                state.data_section_symbols[sym.address] = info;
                
                try {
                    // Get or create appropriate type
                    ghidra::Datatype* dt = nullptr;
                    if (sym.type_meta == 9) {  // TYPE_FLOAT
                        if (sym.size == 8) {
                            dt = types->getBase(8, ghidra::TYPE_FLOAT);  // double
                        } else if (sym.size == 4) {
                            dt = types->getBase(4, ghidra::TYPE_FLOAT);  // float
                        }
                    }
                    
                    if (!dt) {
                        std::cerr << "[fission_decomp] Could not create type for symbol at 0x" 
                                  << std::hex << sym.address << std::endl;
                        continue;
                    }
                    
                    // Create address
                    ghidra::Address addr(ram_space, sym.address);
                    
                    // Check if symbol already exists
                    ghidra::SymbolEntry* existing = global_scope->queryContainer(addr, 1, ghidra::Address());
                    if (existing != nullptr) {
                        continue;
                    }
                    
                    // Add new symbol
                    ghidra::SymbolEntry* entry = global_scope->addSymbol(
                        sym.name,
                        dt,
                        addr,
                        ghidra::Address()  // use point
                    );
                    
                    if (entry) {
                        total_data_symbols++;
                        std::cerr << "[fission_decomp] Registered data symbol: " << sym.name 
                                  << " at 0x" << std::hex << sym.address 
                                  << " type=" << dt->getName() << std::endl;
                    }
                    
                } catch (const std::exception& e) {
                    std::cerr << "[fission_decomp] Exception while registering symbol at 0x" 
                              << std::hex << sym.address << ": " << e.what() << std::endl;
                } catch (...) {
                    std::cerr << "[fission_decomp] Unknown exception while registering symbol at 0x" 
                              << std::hex << sym.address << std::endl;
                }
            }
        }
        
        state.data_symbols_scanned = true;
        std::cerr << "[fission_decomp] Registered " << total_data_symbols 
                  << " data section symbols (cached for future use)" << std::endl;
        
    } else {
        state.loader_64bit->updateData(bin_bytes, image_base);
        state.arch_64bit->symboltab->getGlobalScope()->clear();
    }
    
    // Phase 10: Inject IAT symbols for 64-bit
    auto iat_symbols = extract_iat_symbols(input);
    std::cerr << "[fission_decomp] Parsed " << iat_symbols.size() 
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
        std::vector<std::string> fid_candidates = {
            "./ghidra/funtionID/vs2019_x86.fidbf",
            "./ghidra/funtionID/vs2017_x86.fidbf",
            "./ghidra/funtionID/vs2015_x86.fidbf",
            "./ghidra/funtionID/vs2012_x86.fidbf",
            "../ghidra/funtionID/vs2019_x86.fidbf",
            "../../ghidra/funtionID/vs2019_x86.fidbf"
        };
        
        static FidDatabase fid_db;
        static FunctionMatcher func_matcher;
        static bool fid_initialized = false;
        
        if (!fid_initialized) {
            for (const auto& path : fid_candidates) {
                if (file_exists(path)) {
                    if (fid_db.load(path)) {
                        std::cerr << "[fission_decomp] Loaded FID database: " << path 
                                  << " (" << fid_db.get_function_count() << " functions)" << std::endl;
                        func_matcher.set_fid_database(&fid_db);
                        break;
                    }
                }
            }
            fid_initialized = true;
        }
        
        // Load ALL available FID databases
        static std::vector<FidDatabase> all_fid_dbs;
        if (all_fid_dbs.empty()) {
            std::vector<std::string> all_fidbf = {
                "./ghidra/funtionID/vs2019_x86.fidbf",
                "./ghidra/funtionID/vs2017_x86.fidbf",
                "./ghidra/funtionID/vs2015_x86.fidbf",
                "./ghidra/funtionID/vs2012_x86.fidbf",
                "./ghidra/funtionID/vsOlder_x86.fidbf"
            };
            for (const auto& path : all_fidbf) {
                if (file_exists(path)) {
                    FidDatabase db;
                    if (db.load(path)) {
                        all_fid_dbs.push_back(std::move(db));
                    }
                }
            }
            std::cerr << "[fission_decomp] Loaded " << all_fid_dbs.size() 
                     << " FID databases total" << std::endl;
        }
        
        // Improved function prologue detection
        size_t matched_count = 0;
        std::vector<uint64_t> prologues = PatternLoader::scan_function_prologues(bin_bytes, image_base);
        size_t prologue_count = prologues.size();
        
        for (uint64_t addr : prologues) {
            size_t offset = addr - image_base;
            if (offset + 64 >= bin_bytes.size()) continue;
            
            for (auto& db : all_fid_dbs) {
                func_matcher.set_fid_database(&db);
                std::string name = func_matcher.match_by_fid(addr, &bin_bytes[offset], 64, true);
                if (!name.empty()) {
                    state.fid_function_names[addr] = name;
                    matched_count++;
                    break;
                }
            }
        }
        
        std::cerr << "[fission_decomp] Scanned " << prologue_count << " function prologues" << std::endl;
        if (matched_count > 0) {
            std::cerr << "[fission_decomp] FID matched " << matched_count 
                     << " functions by hash" << std::endl;
        }
        
        // Load common symbols
        std::vector<std::string> symbol_files = {
            "./ghidra/funtionID/common_symbols_win32.txt",
            "./ghidra/funtionID/common_symbols_win64.txt",
            "../ghidra/funtionID/common_symbols_win32.txt"
        };
        
        for (const auto& path : symbol_files) {
            if (file_exists(path)) {
                std::ifstream ifs(path);
                std::string line;
                while (std::getline(ifs, line)) {
                    if (!line.empty() && line[0] != '#') {
                        size_t space = line.find(' ');
                        if (space != std::string::npos && line.substr(0, 2) == "0x") {
                            uint64_t addr = std::stoull(line.substr(2, space - 2), nullptr, 16);
                            std::string name = line.substr(space + 1);
                            state.fid_function_names[addr] = name;
                        }
                    }
                }
                std::cerr << "[fission_decomp] Loaded common symbols from: " << path << std::endl;
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
    std::string compiler_id = "windows";
    
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
    fission::core::CliArchitecture* arch = nullptr;
    
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
    
    std::cerr << "[fission_decomp] Step 1: Clearing global scope for 0x" 
              << std::hex << address << std::dec << std::endl;
    
    // Clear global scope
    ghidra::Scope* global_scope = arch->symboltab->getGlobalScope();
    global_scope->clear();
    
    // FISSION IMPROVEMENT: Re-register cached data section symbols after clear
    if (state.data_symbols_scanned && !state.data_section_symbols.empty()) {
        std::cerr << "[fission_decomp] Re-registering " << state.data_section_symbols.size() 
                  << " cached data section symbols..." << std::endl;
        
        ghidra::TypeFactory* types = arch->types;
        ghidra::AddrSpace* ram_space = arch->getDefaultDataSpace();
        int registered = 0;
        
        for (const auto& pair : state.data_section_symbols) {
            uint64_t addr_val = pair.first;
            const auto& info = pair.second;
            
            try {
                // Get or create appropriate type
                ghidra::Datatype* dt = nullptr;
                if (info.type_meta == 9) {  // TYPE_FLOAT
                    if (info.size == 8) {
                        dt = types->getBase(8, ghidra::TYPE_FLOAT);  // double
                    } else if (info.size == 4) {
                        dt = types->getBase(4, ghidra::TYPE_FLOAT);  // float
                    }
                }
                
                if (!dt) continue;
                
                // Create address and add symbol
                ghidra::Address addr(ram_space, addr_val);
                ghidra::SymbolEntry* entry = global_scope->addSymbol(
                    info.name,
                    dt,
                    addr,
                    ghidra::Address()
                );
                
                if (entry) {
                    registered++;
                }
            } catch (...) {
                // Silently ignore errors during re-registration
            }
        }
        
        std::cerr << "[fission_decomp] Re-registered " << registered 
                  << " data section symbols" << std::endl;
    }
    
    std::cerr << "[fission_decomp] Step 2: Adding function at 0x" 
              << std::hex << address << std::dec << std::endl;
    
    // Add function
    ghidra::Address func_addr(arch->getDefaultCodeSpace(), address);
    ghidra::Funcdata* fd = global_scope->findFunction(func_addr);
    if (fd == nullptr) {
        fd = global_scope->addFunction(func_addr, "func")->getFunction();
    }
    
    std::cerr << "[fission_decomp] Step 3: Resetting actions" << std::endl;
    arch->allacts.getCurrent()->reset(*fd);
    
    // Step 3b: Enforce GDT prototypes
    PrototypeEnforcer proto_enforcer;
    proto_enforcer.enforce_iat_prototypes(arch, state.iat_symbols);
    
    // Step 4: Primary Decompilation
    {
        StepTimer timer("Step 4: Decompilation");
        std::cerr << "[fission_decomp] Step 4: Performing decompilation..." << std::endl;
        arch->allacts.getCurrent()->perform(*fd);
    }
    
    // Step 4b: Advanced Structure Recovery (Guarded)
    std::string inferred_struct_defs;
    std::map<unsigned long long, ghidra::TypeStruct*> captured_structs;
    
    size_t func_size = fd->getSize();
    if (func_size < MAX_FUNCTION_SIZE) {
        StepTimer timer("Step 4b: Structure Recovery");
        StructureAnalyzer struct_analyzer;
        bool structs_found = struct_analyzer.analyze_function_structures(fd);
        
        if (structs_found) {
            std::cerr << "[fission_decomp] Step 4b: New structures inferred! Re-running decompilation..." 
                     << std::endl;
            try {
                inferred_struct_defs = struct_analyzer.generate_struct_definitions();
                captured_structs = struct_analyzer.get_inferred_structs();
                
                fd->clear();
                arch->allacts.getCurrent()->reset(*fd);
                arch->allacts.getCurrent()->perform(*fd);
                
                // Register inferred types
                std::cerr << "[fission_decomp] Registering inferred types for 0x" 
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
                std::cerr << "[fission_decomp] Step 4b ERROR: " << e.explain << std::endl;
            } catch (const std::exception& e) {
                std::cerr << "[fission_decomp] Step 4b EXCEPTION: " << e.what() << std::endl;
            }
        }
        
        // Step 4b-2: Reverse Type Propagation
        StepTimer timer_rev("Step 4b-2: Reverse Propagation");
        // Use TypePropagator with struct registry
        fission::analysis::TypePropagator type_propagator(arch, &global_struct_registry);
        type_propagator.clear();
        bool struct_changed = type_propagator.propagate_struct_types(fd);
        if (struct_changed) {
            std::cerr << "[fission_decomp] Step 4b-2: Reverse propagation applied! Re-running..."
                     << std::endl;
            fd->clear();
            arch->allacts.getCurrent()->reset(*fd);
            arch->allacts.getCurrent()->perform(*fd);
            type_propagator.clear();
        }

        int types_inferred = type_propagator.propagate(fd);
        bool struct_changed_after = type_propagator.propagate_struct_types(fd);
        if (types_inferred > 0 || struct_changed_after) {
            std::cerr << "[fission_decomp] Step 4b-2: Type propagation complete, re-running..."
                     << std::endl;
            fd->clear();
            arch->allacts.getCurrent()->reset(*fd);
            arch->allacts.getCurrent()->perform(*fd);
        }
    } else {
        std::cerr << "[fission_decomp] Step 4b: Skipped (function too large: " 
                  << func_size << " bytes > " << MAX_FUNCTION_SIZE << " limit)" << std::endl;
    }
    
    // Step 4c: Emulation-Assisted Analysis
    EmulationAnalyzer emu_analyzer;
    bool emu_tags_found = emu_analyzer.analyze(fd);
    if (emu_tags_found) {
        std::cerr << "[fission_decomp] Step 4c: Emulation meta-tags added!" << std::endl;
    }
    
    std::cerr << "[fission_decomp] Step 5: Generating output" << std::endl;
    std::ostringstream c_stream;
    arch->print->setOutputStream(&c_stream);
    arch->print->docFunction(fd);
    
    std::cerr << "[fission_decomp] Step 6: Post-processing IAT calls" << std::endl;
    std::string c_code = c_stream.str();
    
    // Inject inferred struct definitions
    if (!inferred_struct_defs.empty()) {
        c_code = inferred_struct_defs + "\n" + c_code;
        c_code = fission::analysis::TypePropagator::apply_struct_types(c_code, fd, captured_structs);
    }
    
    c_code = post_process_iat_calls(c_code, state.iat_symbols);
    c_code = smart_constant_replace(c_code);
    c_code = post_process_constants(c_code, state.enum_values);
    
    // Step 6d: GUID substitution
    if (state.guid_map.empty()) {
        std::vector<std::string> guid_files = {
            "../../ghidra/typeinfo/win32/msvcrt/guids.txt",
            "../ghidra/typeinfo/win32/msvcrt/guids.txt",
            "./ghidra/typeinfo/win32/msvcrt/guids.txt",
            "../../ghidra/typeinfo/win32/msvcrt/iids.txt",
            "../ghidra/typeinfo/win32/msvcrt/iids.txt",
            "./ghidra/typeinfo/win32/msvcrt/iids.txt"
        };
        
        for (const auto& path : guid_files) {
            if (file_exists(path)) {
                std::cerr << "[fission_decomp] Loading GUIDs from: " << path << std::endl;
                std::string content = read_file_content(path);
                if (!content.empty()) {
                    std::map<std::string, std::string> loaded = load_guids_to_map(content);
                    state.guid_map.insert(loaded.begin(), loaded.end());
                }
            }
        }
        if (!state.guid_map.empty()) {
            std::cerr << "[fission_decomp] Loaded " << state.guid_map.size() 
                     << " GUIDs/IIDs." << std::endl;
        }
    }
    c_code = substitute_guids(c_code, state.guid_map);
    
    // Apply remaining post-processors
    c_code = recover_unicode_strings(c_code);
    c_code = replace_interlocked_patterns(c_code);
    c_code = replace_xunknown_types(c_code);
    c_code = cleanup_seh_boilerplate(c_code);
    c_code = improve_internal_function_names(c_code);
    c_code = apply_fid_names(c_code, state.fid_function_names);
    c_code = PostProcessor::convert_integer_constants(c_code);
    
    std::cerr << "[fission_decomp] Step 7: Done!" << std::endl;
    return "{\"status\":\"ok\",\"code\":\"" + json_escape(c_code) + "\"}";
}

} // namespace decompiler
} // namespace fission
