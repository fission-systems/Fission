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

using namespace ghidra;

// Constants
static const int MAX_INSTRUCTIONS = 200000;

// Null buffer to silence logs
class NullBuffer : public std::streambuf {
public:
    int overflow(int c) { return c; }
};
static NullBuffer null_buffer;
static std::ostream null_stream(&null_buffer);

// Simple base64 decoder
static const std::string base64_chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

std::vector<uint8_t> base64_decode(const std::string& encoded) {
    std::vector<uint8_t> result;
    int val = 0, bits = -8;
    for (unsigned char c : encoded) {
        if (c == '=') break;
        size_t pos = base64_chars.find(c);
        if (pos == std::string::npos) {
            // Strict mode: fail on invalid chars (or just skip? User wants robust)
            // Skipping is standard for whitespace, but invalid chars might indicate error.
            // For now, continue to skip to be permissive but predictable.
            continue; 
        }
        val = (val << 6) + pos;
        bits += 6;
        if (bits >= 0) {
            result.push_back((val >> bits) & 0xFF);
            bits -= 8;
        }
    }
    return result;
}

// Simple JSON-like parser (minimal, no external deps)
std::string extract_json_string(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\":\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return "";
    pos += search.length();
    
    // Robust parsing: handle escaped quotes
    size_t end = pos;
    while (end < json.length()) {
        if (json[end] == '"' && end > 0 && json[end-1] != '\\') {
            break;
        }
        end++;
    }
    
    if (end >= json.length()) return ""; // Malformed or not found end quote
    return json.substr(pos, end - pos);
}

int64_t extract_json_int(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return 0;
    pos += search.length();
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;
    std::string num;
    while (pos < json.length() && (isdigit(json[pos]) || json[pos] == '-')) {
        num += json[pos++];
    }
    return num.empty() ? 0 : std::stoll(num);
}

bool extract_json_bool(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return false;
    pos += search.length();
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;
    return (json.substr(pos, 4) == "true");
}

// Escape string for JSON output
std::string json_escape(const std::string& s) {
    std::string result;
    result.reserve(s.size() * 2);
    for (char c : s) {
        switch (c) {
            case '"': result += "\\\""; break;
            case '\\': result += "\\\\"; break;
            case '\n': result += "\\n"; break;
            case '\r': result += "\\r"; break;
            case '\t': result += "\\t"; break;
            default: result += c; break;
        }
    }
    return result;
}

// Extract IAT symbols from JSON object: {"iat_symbols":{"0x401000":"GetProcAddress",...}}
std::map<uint64_t, std::string> extract_iat_symbols(const std::string& json) {
    std::map<uint64_t, std::string> symbols;
    
    // Find "iat_symbols":{
    std::string search = "\"iat_symbols\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return symbols;
    pos += search.length();
    
    // Skip whitespace
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;
    if (pos >= json.length() || json[pos] != '{') return symbols;
    pos++; // skip '{'
    
    // Parse key-value pairs until '}'
    while (pos < json.length()) {
        // Skip whitespace
        while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n')) pos++;
        
        if (pos >= json.length() || json[pos] == '}') break;
        if (json[pos] == ',') { pos++; continue; }
        
        // Parse key (address like "0x401000")
        if (json[pos] != '"') break;
        pos++; // skip opening quote
        size_t key_end = json.find('"', pos);
        if (key_end == std::string::npos) break;
        std::string addr_str = json.substr(pos, key_end - pos);
        pos = key_end + 1;
        
        // Skip ":"
        while (pos < json.length() && (json[pos] == ' ' || json[pos] == ':')) pos++;
        
        // Parse value (function name)
        if (pos >= json.length() || json[pos] != '"') break;
        pos++; // skip opening quote
        size_t val_end = pos;
        while (val_end < json.length()) {
            if (json[val_end] == '"' && val_end > 0 && json[val_end-1] != '\\') break;
            val_end++;
        }
        if (val_end >= json.length()) break;
        std::string func_name = json.substr(pos, val_end - pos);
        pos = val_end + 1;
        
        // Parse address (supports "0x" hex prefix)
        uint64_t addr = 0;
        if (addr_str.substr(0, 2) == "0x" || addr_str.substr(0, 2) == "0X") {
            addr = std::stoull(addr_str.substr(2), nullptr, 16);
        } else {
            addr = std::stoull(addr_str, nullptr, 10);
        }
        
        symbols[addr] = func_name;
    }
    
    return symbols;
}

// Post-process decompiled output to replace indirect calls with IAT function names
// Replaces patterns like "(*pcRam00cf4208)(...)" with "GetProcAddress(...)"
std::string post_process_iat_calls(const std::string& code, const std::map<uint64_t, std::string>& iat_symbols) {
    if (iat_symbols.empty()) return code;
    
    std::string result = code;
    
    // Pattern: (*pcRamXXXXXXXX) or (*pcRam00XXXXXXXX)
    // We look for pcRam followed by hex digits
    for (const auto& [addr, name] : iat_symbols) {
        // Generate patterns for this address (both 32-bit and 64-bit formats)
        char pattern32[32], pattern64[32];
        snprintf(pattern32, sizeof(pattern32), "pcRam%08x", (uint32_t)addr);
        snprintf(pattern64, sizeof(pattern64), "pcRam%016llx", (unsigned long long)addr);
        
        // Also try without leading zeros
        std::ostringstream pattern_stream;
        pattern_stream << "pcRam" << std::hex << std::setfill('0') << std::setw(8) << (addr & 0xFFFFFFFF);
        std::string pattern_lower = pattern_stream.str();
        
        // Replace all occurrences
        size_t pos = 0;
        while ((pos = result.find(pattern32, pos)) != std::string::npos) {
            // Find the start of the dereference pattern (*pcRam...)
            size_t start = pos;
            if (start > 0 && result[start-1] == '*' && start > 1 && result[start-2] == '(') {
                // Find matching closing paren and call args
                size_t end_ptr = result.find(')', start);
                if (end_ptr != std::string::npos) {
                    // Replace "(*pcRam...)" with function name
                    result.replace(start - 2, end_ptr - start + 3, name);
                    pos = start - 2 + name.length();
                    continue;
                }
            }
            pos += strlen(pattern32);
        }
        
        // Also try pattern64 for 64-bit binaries
        pos = 0;
        while ((pos = result.find(pattern64, pos)) != std::string::npos) {
            size_t start = pos;
            if (start > 0 && result[start-1] == '*' && start > 1 && result[start-2] == '(') {
                size_t end_ptr = result.find(')', start);
                if (end_ptr != std::string::npos) {
                    result.replace(start - 2, end_ptr - start + 3, name);
                    pos = start - 2 + name.length();
                    continue;
                }
            }
            pos += strlen(pattern64);
        }
    }
    
    return result;
}

// Custom LoadImage for memory
class MemoryLoadImage : public LoadImage {
    std::vector<uint8_t> data_;
    uint64_t base_addr_;
public:
    MemoryLoadImage(const std::vector<uint8_t>& d, uint64_t base)
        : LoadImage("memory"), data_(d), base_addr_(base) {}
    
    void updateData(const std::vector<uint8_t>& d, uint64_t base) {
        data_ = d;
        base_addr_ = base;
    }
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
        uint64_t max = base_addr_ + data_.size();
        
        // Optimized bulk copy
        if (offset >= base_addr_ && offset + size <= max) {
            std::memcpy(ptr, data_.data() + (offset - base_addr_), size);
        } else {
            // Fallback for boundary crossing
            for(int4 i = 0; i < size; ++i) {
                uint64_t cur = offset + i;
                if (cur >= base_addr_ && cur < max) {
                    ptr[i] = static_cast<uint1>(data_[cur - base_addr_]);
                } else {
                    ptr[i] = 0;
                }
            }
        }
    }
    virtual std::string getArchType(void) const override { return "memory"; }
    virtual void adjustVma(long adjust) override {}
};

// Custom Architecture
class CliArchitecture : public SleighArchitecture {
    MemoryLoadImage* custom_loader;
public:
    CliArchitecture(const std::string& sleigh_id, MemoryLoadImage* ldr, std::ostream* err)
        : SleighArchitecture("", sleigh_id, err), custom_loader(ldr) {}
    
    virtual void buildLoader(DocumentStorage& store) override {
        loader = custom_loader;
    }
};

// Global state for server mode (reuse across requests)
struct ServerState {
    bool initialized = false;
    std::string sla_dir;
    // Cached architecture objects to avoid re-initialization overhead
    MemoryLoadImage* loader_64bit = nullptr;
    MemoryLoadImage* loader_32bit = nullptr;
    CliArchitecture* arch_64bit = nullptr;
    CliArchitecture* arch_32bit = nullptr;
    // Track if architectures have been initialized
    bool arch_64bit_ready = false;
    bool arch_32bit_ready = false;
    // Store IAT symbols for post-processing
    std::map<uint64_t, std::string> iat_symbols;
    
    ~ServerState() {
        if (arch_64bit) delete arch_64bit;
        if (arch_32bit) delete arch_32bit;
        if (loader_64bit) delete loader_64bit;
        if (loader_32bit) delete loader_32bit;
    }
};

// Inject IAT symbols into Ghidra's symbol table
void inject_iat_symbols(CliArchitecture* arch, const std::map<uint64_t, std::string>& symbols) {
    if (!arch || symbols.empty()) return;
    
    Scope* global_scope = arch->symboltab->getGlobalScope();
    if (!global_scope) return;
    
    int injected = 0;
    for (const auto& [addr, name] : symbols) {
        try {
            Address sym_addr(arch->getDefaultCodeSpace(), addr);
            // Get or create function symbol
            Funcdata* existing = global_scope->findFunction(sym_addr);
            if (existing == nullptr) {
                // Create external/import symbol as function
                global_scope->addFunction(sym_addr, name);
                injected++;
            }
        } catch (...) {
            // Ignore symbol injection errors
        }
    }
    
    if (injected > 0) {
        std::cerr << "[fission_decomp] Injected " << injected << " IAT symbols" << std::endl;
    }
}

// Initialize Ghidra (called once in server mode)
bool init_ghidra(ServerState& state, const std::string& sla_dir) {
    if (state.initialized && state.sla_dir == sla_dir) {
        return true;
    }
    
    try {
        startDecompilerLibrary(sla_dir.c_str());
        std::string langDir = sla_dir + "/languages";
        SleighArchitecture::specpaths.addDir2Path(langDir);
        SleighArchitecture::getDescriptions();
        state.sla_dir = sla_dir;
        state.initialized = true;
        return true;
    } catch (...) {
        return false;
    }
}

// Helper to configure architecture safely with advanced options
void configure_arch(CliArchitecture* arch) {
    arch->max_instructions = MAX_INSTRUCTIONS;
    arch->flowoptions &= ~FlowInfo::error_toomanyinstructions;
    
    // === Advanced Ghidra Decompiler Options ===
    // Based on DecompileOptions class from Ghidra API
    
    // 1. Prototype Evaluation Model (calling convention)
    // For Windows x64, the default should be __fastcall-like
    // This affects how parameters and return values are inferred
    
    // 2. Simplification options
    // These are applied during the decompilation action group
    
    // 3. Jump table limits - prevent excessive memory for switch statements
    // Default is usually sufficient
    
    // 4. Read-only memory treatment
    // IAT sections should ideally be marked read-only for proper resolution
    // This is handled by the LoadImage but we can influence through options
    
    // 5. Output formatting options via PrintLanguage base class
    if (arch->print) {
        // Configure output options through base PrintLanguage class
        arch->print->setFlat(false);          // Use indentation
        arch->print->setIndentIncrement(2);   // 2 spaces per indent level
    }
    
    // 6. Alias blocking - helps with type propagation
    // Already enabled by default in standalone mode
    
    // Note: Full DecompileOptions require the Ghidra Java layer
    // In standalone C++ mode, options are more limited but we optimize what we can
}

// Register Windows API types for better type inference
void register_windows_types(CliArchitecture* arch, bool is_64bit) {
    if (!arch) return;
    
    TypeFactory* types = arch->types;
    if (!types) return;
    
    int ptrSize = is_64bit ? 8 : 4;
    
    try {
        // Get base void type for pointer creation
        Datatype* voidType = types->getTypeVoid();
        Datatype* charType = types->getTypeChar(TYPE_INT);
        Datatype* wcharType = types->getBase(2, TYPE_INT); // wchar_t is 2 bytes on Windows
        
        // Create common Windows types
        // Note: Ghidra TypeFactory doesn't allow custom type names directly,
        // but we can create typedef aliases using the architecture's symbol table
        
        // For now, the best approach is to ensure the decompiler uses correct sizes
        // The actual type naming happens through Ghidra's data type archives
        
        // DWORD = uint32
        types->getBase(4, TYPE_UINT);
        
        // WORD = uint16  
        types->getBase(2, TYPE_UINT);
        
        // BYTE = uint8
        types->getBase(1, TYPE_UINT);
        
        // HANDLE = void*
        types->getTypePointer(ptrSize, voidType, 0);
        
        // LPSTR = char*
        types->getTypePointer(ptrSize, charType, 0);
        
        // LPWSTR = wchar_t*
        types->getTypePointer(ptrSize, wcharType, 0);
        
        // BOOL = int32
        types->getBase(4, TYPE_INT);
        
        std::cerr << "[fission_decomp] Windows types registered (" << (is_64bit ? "64" : "32") << "-bit)" << std::endl;
    } catch (...) {
        // Type registration is best-effort
    }
}

// Process a single decompilation request
std::string process_request(ServerState& state, const std::string& input) {
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
            if (!init_ghidra(state, sla_dir)) {
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
                state.arch_64bit = new CliArchitecture("x86:LE:64:default", state.loader_64bit, &null_stream);
                DocumentStorage store;
                state.arch_64bit->init(store);
                configure_arch(state.arch_64bit);
                register_windows_types(state.arch_64bit, true); // 64-bit
                state.arch_64bit_ready = true;
                std::cerr << "[fission_decomp] Initialized 64-bit architecture (persistent)" << std::endl;
            } else {
                 state.loader_64bit->updateData(bin_bytes, image_base);
                 state.arch_64bit->symboltab->getGlobalScope()->clear();
            }
            
            // Inject IAT symbols for 64-bit arch
            auto iat_symbols = extract_iat_symbols(input);
            std::cerr << "[fission_decomp] Parsed " << iat_symbols.size() << " IAT symbols from JSON" << std::endl;
            inject_iat_symbols(state.arch_64bit, iat_symbols);
            // Store for post-processing
            state.iat_symbols = iat_symbols;

            // 32-bit
            if (!state.arch_32bit_ready) {
                if (state.loader_32bit) delete state.loader_32bit;
                if (state.arch_32bit) delete state.arch_32bit;

                state.loader_32bit = new MemoryLoadImage(bin_bytes, image_base);
                state.arch_32bit = new CliArchitecture("x86:LE:32:default", state.loader_32bit, &null_stream);
                DocumentStorage store;
                state.arch_32bit->init(store);
                configure_arch(state.arch_32bit);
                register_windows_types(state.arch_32bit, false); // 32-bit
                state.arch_32bit_ready = true;
                 std::cerr << "[fission_decomp] Initialized 32-bit architecture (persistent)" << std::endl;
            } else {
                 state.loader_32bit->updateData(bin_bytes, image_base);
                 state.arch_32bit->symboltab->getGlobalScope()->clear();
            }
            
            // Inject IAT symbols for 32-bit arch
            inject_iat_symbols(state.arch_32bit, iat_symbols);
            
            return "{\"status\":\"ok\",\"message\":\"Binary loaded\"}";
        }
        
        // Normal Decompile Request
        if (sla_dir.empty()) {
            return "{\"status\":\"error\",\"message\":\"Missing sla_dir\"}";
        }

        if (!init_ghidra(state, sla_dir)) {
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
                 state.arch_64bit = new CliArchitecture("x86:LE:64:default", state.loader_64bit, &null_stream);
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
                 state.arch_32bit = new CliArchitecture("x86:LE:32:default", state.loader_32bit, &null_stream);
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
    
    ServerState state;
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
    
    ServerState state;
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
