/**
 * Fission Decompiler CLI
 * 
 * Standalone subprocess decompiler that reads JSON from stdin and outputs C code to stdout.
 * Each invocation is a fresh process, avoiding Ghidra global state issues.
 * 
 * Input (stdin): {"bytes":"BASE64_ENCODED_BYTES","address":12345,"is_64bit":true,"sla_dir":"/path"}
 * Output (stdout): Decompiled C code or {"error":"message"}
 */

#include <iostream>
#include <sstream>
#include <string>
#include <vector>
#include <cstdint>
#include <cstring>

#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "flow.hh"

using namespace ghidra;

// Simple base64 decoder
static const std::string base64_chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

std::vector<uint8_t> base64_decode(const std::string& encoded) {
    std::vector<uint8_t> result;
    int val = 0, bits = -8;
    for (unsigned char c : encoded) {
        if (c == '=') break;
        size_t pos = base64_chars.find(c);
        if (pos == std::string::npos) continue;
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
    size_t end = json.find("\"", pos);
    if (end == std::string::npos) return "";
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

// Custom LoadImage for memory
class MemoryLoadImage : public LoadImage {
    std::vector<uint8_t> data_;
    uint64_t base_addr_;
public:
    MemoryLoadImage(const std::vector<uint8_t>& d, uint64_t base)
        : LoadImage("memory"), data_(d), base_addr_(base) {}
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
        uint64_t max = base_addr_ + data_.size();
        for(int4 i = 0; i < size; ++i) {
            uint64_t cur = offset + i;
            if (cur >= base_addr_ && cur < max) {
                ptr[i] = static_cast<uint1>(data_[cur - base_addr_]);
            } else {
                ptr[i] = 0;
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

int main(int argc, char** argv) {
    // Read all of stdin
    std::stringstream buffer;
    buffer << std::cin.rdbuf();
    std::string input = buffer.str();
    
    if (input.empty()) {
        std::cerr << "{\"error\":\"No input provided\"}" << std::endl;
        return 1;
    }
    
    // Parse JSON input
    std::string bytes_b64 = extract_json_string(input, "bytes");
    int64_t address = extract_json_int(input, "address");
    bool is_64bit = extract_json_bool(input, "is_64bit");
    std::string sla_dir = extract_json_string(input, "sla_dir");
    
    if (bytes_b64.empty() || sla_dir.empty()) {
        std::cerr << "{\"error\":\"Missing required fields: bytes, sla_dir\"}" << std::endl;
        return 1;
    }
    
    // Decode bytes
    std::vector<uint8_t> bytes = base64_decode(bytes_b64);
    if (bytes.empty()) {
        std::cerr << "{\"error\":\"Failed to decode bytes\"}" << std::endl;
        return 1;
    }
    
    try {
        // Initialize Ghidra
        startDecompilerLibrary(sla_dir.c_str());
        
        std::string langDir = sla_dir + "/languages";
        SleighArchitecture::specpaths.addDir2Path(langDir);
        SleighArchitecture::getDescriptions();
        
        // Select architecture
        const char* arch_id = is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
        
        // Create loader and architecture
        MemoryLoadImage loader(bytes, address);
        CliArchitecture arch(arch_id, &loader, &std::cerr);
        
        DocumentStorage store;
        arch.init(store);
        arch.max_instructions = 200000;
        arch.flowoptions &= ~FlowInfo::error_toomanyinstructions;
        
        // Decompile
        Address func_addr(arch.getDefaultCodeSpace(), address);
        Scope* global_scope = arch.symboltab->getGlobalScope();
        Funcdata* fd = global_scope->findFunction(func_addr);
        if (fd == nullptr) {
            fd = global_scope->addFunction(func_addr, "func")->getFunction();
        }
        
        arch.allacts.getCurrent()->reset(*fd);
        arch.allacts.getCurrent()->perform(*fd);
        
        std::ostringstream c_stream;
        arch.print->setOutputStream(&c_stream);
        arch.print->docFunction(fd);
        
        // Output result
        std::cout << c_stream.str();
        return 0;
        
    } catch (const LowlevelError& e) {
        std::cerr << "{\"error\":\"" << e.explain << "\"}" << std::endl;
        return 1;
    } catch (const std::exception& e) {
        std::cerr << "{\"error\":\"" << e.what() << "\"}" << std::endl;
        return 1;
    }
}
