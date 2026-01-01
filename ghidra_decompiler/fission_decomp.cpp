/**
 * Fission Decompiler CLI
 * 
 * Standalone subprocess decompiler that reads JSON from stdin and outputs C code to stdout.
 * 
 * Modes:
 *   - Single-shot (default): Process one request and exit
 *   - Server (--server): Keep running and process multiple requests (line-delimited JSON)
 * 
 * Input (stdin): {"bytes":"BASE64_ENCODED_BYTES","address":12345,"is_64bit":true,"sla_dir":"/path"}
 * Output (stdout): {"status":"ok","code":"..."} or {"status":"error","message":"..."}
 */

#include <iostream>
#include <sstream>
#include <string>
#include <vector>
#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <memory>
#include <algorithm>
#include <iomanip>
#include <map>

#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "flow.hh"
#include "type.hh"
#include <fstream>

// Fission modules
#include "fission/processing/Constants.h"
#include "fission/processing/PostProcessors.h"
#include "fission/utils/encoding.h"
#include "fission/utils/json_utils.h"
#include "fission/utils/file_utils.h"
#include "fission/utils/logger.h"
#include "fission/loader/MemoryImage.h"

using namespace ghidra;
using namespace fission::utils;
using namespace fission::loader;
#include "fission/types/TypeManager.h"
#include "fission/core/CliArchitecture.h"
#include "fission/core/DecompilerContext.h"
using namespace fission::types;
using namespace fission::core;
using namespace fission::core;
using namespace fission::processing;
using namespace fission::types;
#include "fission/types/GuidParser.h"
#include "fission/types/RttiAnalyzer.h"
#include "fission/loader/PeHeader.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/processing/StringScanner.h"
#include "fission/types/PatternLoader.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/loader/SymbolLoader.h"
#include "fission/analysis/EmulationAnalyzer.h"
#include "fission/types/PrototypeEnforcer.h"
using namespace fission::loader;
using namespace fission::analysis;

// Constants
static const int MAX_INSTRUCTIONS = 200000;

// Helper function to select FID database
std::string get_fid_filename(bool is_64bit, const std::string& compiler_id) {
    std::string suffix = is_64bit ? "_x64.fidbf" : "_x86.fidbf";
    std::string fid_filename = "vs2019" + suffix; // Default

    if (compiler_id.find("vs2017") != std::string::npos) fid_filename = "vs2017" + suffix;
    else if (compiler_id.find("vs2015") != std::string::npos) fid_filename = "vs2015" + suffix;
    else if (compiler_id.find("vs2012") != std::string::npos) fid_filename = "vs2012" + suffix;
    
    return fid_filename;
}

// Process a single decompilation request
std::string process_request(DecompilerContext& state, const std::string& input) {
    // Debug log: show request type
    std::cerr << "[fission_decomp] Received request: " << input.substr(0, std::min(input.size(), (size_t)100)) << "..." << std::endl;
    
    // Check for special commands
    std::string cmd = extract_json_string(input, "cmd");
    if (cmd == "quit") {
        return "__QUIT__";
    }
    if (cmd == "ping") {
        return "{\"status\":\"ok\",\"message\":\"pong\"}";
    }
    
    try {
        // Parse JSON input
        std::string bytes_b64 = extract_json_string(input, "bytes");
        int64_t address = extract_json_int(input, "address");
        bool is_64bit = extract_json_bool(input, "is_64bit");
        std::string sla_dir = extract_json_string(input, "sla_dir");
        std::string load_bin_cmd = extract_json_string(input, "load_bin");
        std::string gdt_json = extract_json_string(input, "gdt_json"); // GDT types JSON path
        std::string compiler_id = "windows"; // Default compiler ID

        // Handle "load_bin" command
        if (!load_bin_cmd.empty()) {
            std::vector<uint8_t> bin_bytes = base64_decode(load_bin_cmd);
            if (bin_bytes.empty()) {
                return "{\"status\":\"error\",\"message\":\"Failed to decode load_bin bytes\"}";
            }
            
            // Parse image_base (critical for correct address calculation!)
            int64_t image_base = extract_json_int(input, "image_base");
            std::cerr << "[fission_decomp] load_bin: size=" << bin_bytes.size() << " image_base=0x" << std::hex << image_base << std::dec << std::endl;
            
            // Debug: Print first 16 bytes to verify binary data
            std::cerr << "[fission_decomp] First 16 bytes: ";
            for (size_t i = 0; i < std::min((size_t)16, bin_bytes.size()); ++i) {
                std::cerr << std::hex << std::setw(2) << std::setfill('0') << (int)bin_bytes[i] << " ";
            }
            std::cerr << std::dec << std::endl;
            
            // Initialize Ghidra if needed
            if (sla_dir.empty()) {
                return "{\"status\":\"error\",\"message\":\"Missing sla_dir for load_bin\"}";
            }
            if (!state.initialize(sla_dir)) {
                return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
            }

            // Initialize/Update loaders for both architectures with complete binary
            // Use image_base as the base address for correct address translation!
            
            std::cerr << "[fission_decomp] Debug: Detecting PE Arch..." << std::endl;
            // PE Auto-Detection
            bool is_pe = false;
            std::string compiler_id = "windows"; // Default to windows for better safety
            PeDetectionResult pe_info = detect_pe_arch(bin_bytes);
            std::cerr << "[fission_decomp] Debug: PE Detection Result: is_pe=" << pe_info.is_pe << std::endl;
            
            if (pe_info.is_pe) {
                is_pe = true;
                if (!pe_info.compiler_id.empty()) {
                     compiler_id = pe_info.compiler_id;
                }
                std::cerr << "[fission_decomp] Detected PE Binary: " << (pe_info.is_64bit ? "64-bit" : "32-bit") 
                          << " Compiler: " << compiler_id << std::endl;
            }

            // 64-bit
            if (!state.arch_64bit_ready) {
                // Safety: delete if exists (defensive programming)
                if (state.loader_64bit) delete state.loader_64bit;
                if (state.arch_64bit) delete state.arch_64bit;
                
                // RTTI Recovery
                if (is_pe) {
                    std::cerr << "[fission_decomp] Debug: Running RTTI Recovery..." << std::endl;
                    std::map<uint64_t, std::string> recovered_classes = RttiAnalyzer::recover_class_names(bin_bytes, image_base, pe_info.is_64bit);
                    if (!recovered_classes.empty()) {
                        std::cerr << "[fission_decomp] Recovered " << recovered_classes.size() << " class names via RTTI." << std::endl;
                    }
                    
                    // Phase 7: PDB Symbol Loading
                    if (!pe_info.pdb_path.empty()) {
                         // ... (keep existing logic) ...
                    }
                }
                
                std::cerr << "[fission_decomp] Debug: Loading Patterns..." << std::endl;
                // Phase 7: Pattern Matching (FID Lite)
                auto patterns = PatternLoader::load_standard_patterns();
                
                std::cerr << "[fission_decomp] Debug: Running Pattern Matching..." << std::endl;
                auto matches = PatternLoader::match_functions(bin_bytes, image_base, patterns);
                if (!matches.empty()) {
                    std::cerr << "[fission_decomp] Identified " << matches.size() << " standard library functions via patterns." << std::endl;
                    state.iat_symbols.insert(matches.begin(), matches.end());
                }

                // Phase 8: Full FID Analysis (Ghidra Function ID)
                std::string fid_filename;
                if (is_pe) {
                    fid_filename = get_fid_filename(pe_info.is_64bit, compiler_id);
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
                                // Scan for function prologues and hash
                                // Simple heuristic scan
                                size_t step = 1;
                                for (size_t i = 0; i < bin_bytes.size() - 32; i += step) {
                                    bool possible_start = false;
                                    if (pe_info.is_64bit) {
                                        // x64 Prologues
                                        uint8_t b0 = bin_bytes[i];
                                        uint8_t b1 = bin_bytes[i+1];
                                        // push rbx/rbp/rsi/rdi (40 53/55/56/57)
                                        if (b0 == 0x40 && (b1 == 0x53 || b1 == 0x55 || b1 == 0x56 || b1 == 0x57)) possible_start = true;
                                        // sub rsp, imm (48 83 EC / 48 81 EC)
                                        if (b0 == 0x48 && (b1 == 0x83 || b1 == 0x81) && bin_bytes[i+2] == 0xEC) possible_start = true;
                                        // mov [rsp+...], reg (48 89 ...)
                                        if (b0 == 0x48 && b1 == 0x89) possible_start = true;
                                    } else {
                                        // x86 Prologues
                                        // push ebp; mov ebp, esp (55 8B EC)
                                        if (bin_bytes[i] == 0x55 && bin_bytes[i+1] == 0x8B && bin_bytes[i+2] == 0xEC) possible_start = true;
                                        // sub esp, imm (83 EC / 81 EC)
                                        if ((bin_bytes[i] == 0x83 || bin_bytes[i] == 0x81) && bin_bytes[i+1] == 0xEC) possible_start = true;
                                    }

                                    if (possible_start) {
                                        size_t len = std::min((size_t)64, bin_bytes.size() - i);
                                        uint64_t hash = FidHasher::calculate_full_hash(&bin_bytes[i], len);
                                        auto names = fid_db.lookup_by_hash(hash);
                                        if (!names.empty()) {
                                            uint64_t addr = image_base + i;
                                            // Use the first name, but prefer non-generic ones if possible
                                            std::string name = names[0];
                                            // Avoid overwriting if we already have a better name (e.g. from exports)
                                            if (state.iat_symbols.find(addr) == state.iat_symbols.end()) {
                                                state.iat_symbols[addr] = name;
                                                matches_found++;
                                            }
                                        }
                                    }
                                }
                                std::cerr << "[fission_decomp] FID Analysis: Identified " << matches_found << " functions." << std::endl;
                            }
                            break; // Loaded one DB, stop
                        }
                    }
                }
                
                std::cerr << "[fission_decomp] Debug: String Scanning..." << std::endl;
                // String Scanning (Phase 6)
                auto ascii_strings = StringScanner::scan_ascii_strings(bin_bytes, image_base);
                auto unicode_strings = StringScanner::scan_unicode_strings(bin_bytes, image_base);
                std::cerr << "[fission_decomp] Scanned " << ascii_strings.size() << " ASCII and " << unicode_strings.size() << " Unicode strings." << std::endl;
                
                // Store in DecompilerContext for post-processing
                state.enum_values.insert(ascii_strings.begin(), ascii_strings.end());
                state.enum_values.insert(unicode_strings.begin(), unicode_strings.end());

                std::cerr << "[fission_decomp] Debug: Creating Loader and Arch..." << std::endl;
                // Use centralized architecture setup
                state.setup_architecture(true, bin_bytes, image_base, compiler_id);
            } else {
                 state.loader_64bit->updateData(bin_bytes, image_base);
                 state.arch_64bit->symboltab->getGlobalScope()->clear();
            }
            
            // Inject IAT symbols for 64-bit arch
            auto iat_symbols = extract_iat_symbols(input);
            std::cerr << "[fission_decomp] Parsed " << iat_symbols.size() << " IAT symbols from JSON" << std::endl;
            state.arch_64bit->injectIatSymbols(iat_symbols);
            // Store for post-processing
            state.iat_symbols = iat_symbols;
            
            // 32-bit
            if (!state.arch_32bit_ready) {
                // Use centralized architecture setup
                state.setup_architecture(false, bin_bytes, image_base, compiler_id);
            } else {
                 state.loader_32bit->updateData(bin_bytes, image_base);
                 state.arch_32bit->symboltab->getGlobalScope()->clear();
            }
            
            // Inject IAT symbols for 32-bit arch
            state.arch_32bit->injectIatSymbols(iat_symbols);
            
            // Load FID databases for function identification
            {
                std::vector<std::string> fid_candidates = {
                    "./ghidra/funtionID/vs2019_x86.fidbf",
                    "./ghidra/funtionID/vs2017_x86.fidbf",
                    "./ghidra/funtionID/vs2015_x86.fidbf",
                    "./ghidra/funtionID/vs2012_x86.fidbf",
                    "../ghidra/funtionID/vs2019_x86.fidbf",
                    "../../ghidra/funtionID/vs2019_x86.fidbf"
                };
                
                static fission::analysis::FidDatabase fid_db;
                static fission::analysis::FunctionMatcher func_matcher;
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
                
                // Load ALL available FID databases for better coverage
                static std::vector<fission::analysis::FidDatabase> all_fid_dbs;
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
                            fission::analysis::FidDatabase db;
                            if (db.load(path)) {
                                all_fid_dbs.push_back(std::move(db));
                            }
                        }
                    }
                    std::cerr << "[fission_decomp] Loaded " << all_fid_dbs.size() << " FID databases total" << std::endl;
                }
                
                // Improved function prologue detection
                // Scan every 16 bytes (much denser than 4KB) for function prologues
                size_t matched_count = 0;
                
                std::vector<uint64_t> prologues = PatternLoader::scan_function_prologues(bin_bytes, image_base);
                size_t prologue_count = prologues.size();
                
                for (uint64_t addr : prologues) {
                    size_t offset = addr - image_base;
                    if (offset + 64 >= bin_bytes.size()) continue;

                    // Try each FID database
                    for (auto& db : all_fid_dbs) {
                        func_matcher.set_fid_database(&db);
                        std::string name = func_matcher.match_by_fid(addr, &bin_bytes[offset], 64, true);
                        if (!name.empty()) {
                            state.fid_function_names[addr] = name;
                            matched_count++;
                            break;  // Found match, don't try other DBs
                        }
                    }
                }
                
                std::cerr << "[fission_decomp] Scanned " << prologue_count << " function prologues" << std::endl;
                if (matched_count > 0) {
                    std::cerr << "[fission_decomp] FID matched " << matched_count << " functions by hash" << std::endl;
                }
                
                // Load common symbols (well-known function names) - fallback
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
                            // Format: address name (or just name)
                            if (!line.empty() && line[0] != '#') {
                                // Parse simple format: 0xADDRESS NAME or NAME
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
        
        // Normal Decompile Request
        if (sla_dir.empty()) {
            return "{\"status\":\"error\",\"message\":\"Missing sla_dir\"}";
        }

        if (!state.initialize(sla_dir)) {
            return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
        }

        // Determine loader/arch
        MemoryLoadImage* loader = nullptr;
        CliArchitecture* arch = nullptr;

        std::vector<uint8_t> bytes;
        if (!bytes_b64.empty()) bytes = base64_decode(bytes_b64);

        // Use centralized architecture setup
        state.setup_architecture(is_64bit, bytes, address, compiler_id);

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
        
        std::cerr << "[fission_decomp] Step 1: Clearing global scope for 0x" << std::hex << address << std::dec << std::endl;
        
        // Clear global scope to ensure no zombie Function objects remain
        // Warning: This is NOT thread safe currently, but CLI process is single-threaded.
        Scope* global_scope = arch->symboltab->getGlobalScope();
        global_scope->clear();

        std::cerr << "[fission_decomp] Step 2: Adding function at 0x" << std::hex << address << std::dec << std::endl;
        
        // Decompile the new function
        Address func_addr(arch->getDefaultCodeSpace(), address);
        Funcdata* fd = global_scope->findFunction(func_addr);
        if (fd == nullptr) {
            fd = global_scope->addFunction(func_addr, "func")->getFunction();
        }

        std::cerr << "[fission_decomp] Step 3: Resetting actions" << std::endl;
        arch->allacts.getCurrent()->reset(*fd);

        // Step 3b: Enforce GDT prototypes on IAT symbols
        fission::types::PrototypeEnforcer proto_enforcer;
        proto_enforcer.enforce_iat_prototypes(arch, state.iat_symbols);
        
        std::cerr << "[fission_decomp] Step 4: Performing decompilation (this may take time)..." << std::endl;
        arch->allacts.getCurrent()->perform(*fd);

        // Step 4b: Advanced Structure Recovery (Auto-Struct)
        // Now fixed: Uses unique per-function type names and arch-specific sizes.
        // Only triggers re-decompilation when NEW types are created (not reused).
        fission::types::StructureAnalyzer struct_analyzer;
        bool structs_found = struct_analyzer.analyze_function_structures(fd);
        
        if (structs_found) {
             std::cerr << "[fission_decomp] Step 4b: New structures inferred! Re-running decompilation..." << std::endl;
             try {
                 // Clear the function state to reset processing_started flag
                 fd->clear();
                 // Note: Don't call startProcessing() - perform() handles it internally
                 arch->allacts.getCurrent()->reset(*fd);
                 arch->allacts.getCurrent()->perform(*fd);
             } catch (const LowlevelError& e) {
                 std::cerr << "[fission_decomp] Step 4b ERROR: " << e.explain << std::endl;
                 // Continue with original decompilation result
             } catch (const std::exception& e) {
                 std::cerr << "[fission_decomp] Step 4b EXCEPTION: " << e.what() << std::endl;
             }
        }

        // Step 4c: Emulation-Assisted Analysis (Hyper-Context Tagging)
        // Run a trace emulation to tag conditional branches and loops
        fission::analysis::EmulationAnalyzer emu_analyzer;
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
        c_code = post_process_iat_calls(c_code, state.iat_symbols);
        
        // Step 6b: Smart constant replacement (context-aware based on API parameters)
        c_code = smart_constant_replace(c_code);
        
        // Step 6c: Fallback constant replacement for remaining constants
        c_code = post_process_constants(c_code, state.enum_values);
        
        // Step 6d: GUID substitution
        // Load GUIDs if not already loaded (global check)
        if (state.guid_map.empty()) {
             // Autoload GUIDs from ghidra/typeinfo/win32/msvcrt/guids.txt and iids.txt
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
                 std::cerr << "[fission_decomp] Loaded " << state.guid_map.size() << " GUIDs/IIDs." << std::endl;
             }
        }
        c_code = substitute_guids(c_code, state.guid_map);
        
        // Step 6e: Unicode String Recovery
        c_code = recover_unicode_strings(c_code);
        
        // Step 6f: Interlocked Pattern Replacement
        c_code = replace_interlocked_patterns(c_code);
        
        // Step 6g: xunknown/undefined Type Replacement
        c_code = replace_xunknown_types(c_code);
        
        // Step 6h: SEH Boilerplate Cleanup
        c_code = cleanup_seh_boilerplate(c_code);
        
        // Step 6i: Internal Function Naming
        c_code = improve_internal_function_names(c_code);
        
        // Step 6j: Apply FID-resolved function names
        c_code = apply_fid_names(c_code, state.fid_function_names);
        
        std::cerr << "[fission_decomp] Step 7: Done!" << std::endl;
        return "{\"status\":\"ok\",\"code\":\"" + json_escape(c_code) + "\"}";
        
    } catch (const LowlevelError& e) {
        return "{\"status\":\"error\",\"message\":\"" + json_escape(e.explain) + "\"}";
    } catch (const std::exception& e) {
        return "{\"status\":\"error\",\"message\":\"" + json_escape(e.what()) + "\"}";
    } catch (...) {
        return "{\"status\":\"error\",\"message\":\"Unknown Ghidra error\"}";
    }
}

// Server mode: process multiple requests
int run_server() {
    std::cerr << "[fission_decomp] Server mode started" << std::endl;
    std::cout.setf(std::ios::unitbuf);
    
    DecompilerContext state;
    std::string line;
    
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;
        
        std::string response = process_request(state, line);
        
        if (response == "__QUIT__") {
            std::cout << "{\"status\":\"ok\",\"message\":\"goodbye\"}" << std::endl;
            break;
        }
        
        std::cout << response << std::endl;
        std::cout.flush();
    }
    
    std::cerr << "[fission_decomp] Server shutting down" << std::endl;
    return 0;
}

// Single-shot mode: process one request and exit
int run_single() {
    std::cout.setf(std::ios::unitbuf);
    
    // Read all of stdin
    std::stringstream buffer;
    buffer << std::cin.rdbuf();
    std::string input = buffer.str();
    
    if (input.empty()) {
        std::cout << "{\"status\":\"error\",\"message\":\"No input provided\"}" << std::endl;
        return 1;
    }
    
    DecompilerContext state;
    std::string response = process_request(state, input);
    std::cout << response << std::endl;
    std::cout.flush();
    _exit(0);  // Skip cleanup to avoid Ghidra memory corruption crash
}

int main(int argc, char** argv) {
    // Check for --server flag
    bool server_mode = false;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--server") == 0 || strcmp(argv[i], "-s") == 0) {
            server_mode = true;
            break;
        }
    }
    
    if (server_mode) {
        return run_server();
    } else {
        return run_single();
    }
}
