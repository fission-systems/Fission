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
#include "fission/types/GdtParser.h"
#include "fission/types/TypeManager.h"
#include "fission/core/CliArchitecture.h"
#include "fission/core/DecompilerContext.h"
using namespace fission::types;
using namespace fission::core;
using namespace fission::processing;

// Constants
static const int MAX_INSTRUCTIONS = 200000;




// GDT logic moved to fission/types module




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

        // Handle "load_bin" command
        if (!load_bin_cmd.empty()) {
            std::vector<uint8_t> bin_bytes = base64_decode(load_bin_cmd);
            if (bin_bytes.empty()) {
                return "{\"status\":\"error\",\"message\":\"Failed to decode load_bin bytes\"}";
            }
            
            // Parse image_base (critical for correct address calculation!)
            int64_t image_base = extract_json_int(input, "image_base");
            std::cerr << "[fission_decomp] load_bin: size=" << bin_bytes.size() << " image_base=0x" << std::hex << image_base << std::dec << std::endl;
            
            // Initialize Ghidra if needed
            if (sla_dir.empty()) {
                return "{\"status\":\"error\",\"message\":\"Missing sla_dir for load_bin\"}";
            }
            if (!state.initialize(sla_dir)) {
                return "{\"status\":\"error\",\"message\":\"Failed to initialize Ghidra\"}";
            }

            // Initialize/Update loaders for both architectures with complete binary
            // Use image_base as the base address for correct address translation!
            
            // 64-bit
            if (!state.arch_64bit_ready) {
                // Safety: delete if exists (defensive programming)
                if (state.loader_64bit) delete state.loader_64bit;
                if (state.arch_64bit) delete state.arch_64bit;
                
                state.loader_64bit = new MemoryLoadImage(bin_bytes, image_base);
                state.arch_64bit = new CliArchitecture("x86:LE:64:default", state.loader_64bit, &fission::utils::null_stream());
                DocumentStorage store;
                state.arch_64bit->init(store);
                configure_arch(state.arch_64bit);
                TypeManager::register_windows_types(state.arch_64bit->types, 8); // 64-bit
                // Load GDT types if provided (64-bit)
                if (!gdt_json.empty()) {
                    std::string content = read_file_content(gdt_json);
                    if (!content.empty()) {
                        GdtData data = parse_gdt_json(content);
                        TypeManager::load_gdt_types(state.arch_64bit->types, data, 8);
                    }
                }
                state.arch_64bit_ready = true;
                std::cerr << "[fission_decomp] Initialized 64-bit architecture (persistent)" << std::endl;
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
            
            // Load enum values for constant substitution (from 64-bit GDT)
            if (!gdt_json.empty() && state.enum_values.empty()) {
                std::string content = read_file_content(gdt_json);
                if (!content.empty()) {
                   GdtData data = parse_gdt_json(content);
                   state.enum_values = load_gdt_enums(data);
                }
            }

            // 32-bit
            if (!state.arch_32bit_ready) {
                if (state.loader_32bit) delete state.loader_32bit;
                if (state.arch_32bit) delete state.arch_32bit;

                state.loader_32bit = new MemoryLoadImage(bin_bytes, image_base);
                state.arch_32bit = new CliArchitecture("x86:LE:32:default", state.loader_32bit, &fission::utils::null_stream());
                DocumentStorage store;
                state.arch_32bit->init(store);
                configure_arch(state.arch_32bit);
                TypeManager::register_windows_types(state.arch_32bit->types, 4); // 32-bit
                // Load GDT types if provided (use 32-bit GDT for 32-bit arch if available)
                if (!gdt_json.empty()) {
                    // Try to find 32-bit version by replacing "64" with "32" in path
                    std::string gdt_json_32 = gdt_json;
                    size_t pos = gdt_json_32.find("64");
                    if (pos != std::string::npos) {
                        gdt_json_32.replace(pos, 2, "32");
                    }
                    std::string content = read_file_content(gdt_json_32);
                    if (!content.empty()) {
                        GdtData data = parse_gdt_json(content);
                        TypeManager::load_gdt_types(state.arch_32bit->types, data, 4);
                    }
                }
                state.arch_32bit_ready = true;
                 std::cerr << "[fission_decomp] Initialized 32-bit architecture (persistent)" << std::endl;
            } else {
                 state.loader_32bit->updateData(bin_bytes, image_base);
                 state.arch_32bit->symboltab->getGlobalScope()->clear();
            }
            
            // Inject IAT symbols for 32-bit arch
            state.arch_32bit->injectIatSymbols(iat_symbols);
            
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

        if (is_64bit) {
            if (!state.arch_64bit_ready) {
                 // Fallback initialization
                 std::vector<uint8_t> bytes;
                 if (!bytes_b64.empty()) bytes = base64_decode(bytes_b64);
                 
                 if (state.loader_64bit) delete state.loader_64bit;
                 if (state.arch_64bit) delete state.arch_64bit;

                 state.loader_64bit = new MemoryLoadImage(bytes, address);
                 state.arch_64bit = new CliArchitecture("x86:LE:64:default", state.loader_64bit, &fission::utils::null_stream());
                 DocumentStorage store;
                 state.arch_64bit->init(store);
                 configure_arch(state.arch_64bit);
                 state.arch_64bit_ready = true;
            }
            loader = state.loader_64bit;
            arch = state.arch_64bit;
        } else {
            if (!state.arch_32bit_ready) {
                 std::vector<uint8_t> bytes;
                 if (!bytes_b64.empty()) bytes = base64_decode(bytes_b64);
                 
                 if (state.loader_32bit) delete state.loader_32bit;
                 if (state.arch_32bit) delete state.arch_32bit;

                 state.loader_32bit = new MemoryLoadImage(bytes, address);
                 state.arch_32bit = new CliArchitecture("x86:LE:32:default", state.loader_32bit, &fission::utils::null_stream());
                 DocumentStorage store;
                 state.arch_32bit->init(store);
                 configure_arch(state.arch_32bit);
                 state.arch_32bit_ready = true;
            }
            loader = state.loader_32bit;
            arch = state.arch_32bit;
        }

        if (!bytes_b64.empty()) {
            std::vector<uint8_t> bytes = base64_decode(bytes_b64);
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
        
        std::cerr << "[fission_decomp] Step 4: Performing decompilation (this may take time)..." << std::endl;
        arch->allacts.getCurrent()->perform(*fd);

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
