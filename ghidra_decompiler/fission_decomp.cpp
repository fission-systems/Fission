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
#include "fission/constants.h"
#include "fission/post_processors.h"

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
// Simple JSON-like parser (minimal, no external deps)
std::string extract_json_string(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return "";
    pos += search.length();
    
    // Find colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != ':') return "";
    pos++;
    
    // Find opening quote
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != '"') return "";
    pos++;
    
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
    std::string search = "\"" + key + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return 0;
    pos += search.length();
    
    // Find colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != ':') return 0;
    pos++;
    
    // Skip whitespace after colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    
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
    // Store enum/constant values for constant name substitution (value -> name)
    std::map<uint64_t, std::string> enum_values;
    
    ~ServerState() {
        if (arch_64bit) delete arch_64bit;
        if (arch_32bit) delete arch_32bit;
        if (loader_64bit) delete loader_64bit;
        if (loader_32bit) delete loader_32bit;
    }
};

// Structure to hold GDT type information  
struct GdtField {
    std::string name;
    int offset;
    int size;
};

struct GdtStruct {
    std::string name;
    int size;
    int alignment;
    std::vector<GdtField> fields;
};

struct GdtTypedef {
    std::string alias;
    std::string base;
};

struct GdtEnum {
    std::string name;
    uint64_t value;
};

struct GdtData {
    std::vector<GdtStruct> structs;
    std::vector<GdtTypedef> typedefs;
    std::vector<GdtEnum> enums;
};

// ============================================================================
// resolve_gdt_type: "Smart" type resolver for GDT -> Ghidra type conversion
// ============================================================================
// This function handles two main problems:
// 1. Primitive type name mismatch (GDT uses C names, Ghidra uses internal names)
// 2. Pointer type parsing (GDT uses "char *", Ghidra needs dynamic pointer creation)
//
// Returns: Datatype* if resolved, nullptr if not found
// ============================================================================
Datatype* resolve_gdt_type(TypeFactory* types, const std::string& gdt_type_name, int ptr_size) {
    if (!types || gdt_type_name.empty()) return nullptr;

    std::string type_name = gdt_type_name;

    // Trim whitespace
    while (!type_name.empty() && (type_name.front() == ' ' || type_name.front() == '\t')) {
        type_name.erase(type_name.begin());
    }
    while (!type_name.empty() && (type_name.back() == ' ' || type_name.back() == '\t')) {
        type_name.pop_back();
    }

    if (type_name.empty()) return nullptr;

    // =========================================================================
    // Step 1: Handle pointer types (e.g., "char *", "void *", "int **")
    // =========================================================================
    int pointer_depth = 0;
    while (!type_name.empty() && type_name.back() == '*') {
        pointer_depth++;
        type_name.pop_back();
        // Trim trailing space before the *
        while (!type_name.empty() && (type_name.back() == ' ' || type_name.back() == '\t')) {
            type_name.pop_back();
        }
    }

    // =========================================================================
    // Step 2: Map C primitive types to Ghidra internal types
    // =========================================================================
    // GDT (C-style)          -> Ghidra internal name
    // ---------------------------------------------------
    // void                   -> void
    // char                   -> char (or int1 for signed)
    // unsigned char          -> uint1 / byte
    // short                  -> int2
    // unsigned short         -> uint2
    // int                    -> int4
    // unsigned int           -> uint4
    // long                   -> int4 (Windows: 4 bytes)
    // unsigned long          -> uint4
    // long long              -> int8
    // unsigned long long     -> uint8
    // __int64                -> int8
    // unsigned __int64       -> uint8
    // float                  -> float4
    // double                 -> float8
    // wchar_t                -> wchar (or int2 on Windows)
    // =========================================================================

    Datatype* base_type = nullptr;

    // Check for "unsigned" prefix
    bool is_unsigned = false;
    if (type_name.substr(0, 9) == "unsigned ") {
        is_unsigned = true;
        type_name = type_name.substr(9);
        // Trim again
        while (!type_name.empty() && type_name.front() == ' ') {
            type_name.erase(type_name.begin());
        }
    }

    // Check for "signed" prefix (rarely used but valid)
    if (type_name.substr(0, 7) == "signed ") {
        is_unsigned = false;
        type_name = type_name.substr(7);
        while (!type_name.empty() && type_name.front() == ' ') {
            type_name.erase(type_name.begin());
        }
    }

    // Map to Ghidra types
    if (type_name == "void") {
        base_type = types->getTypeVoid();
    }
    else if (type_name == "char") {
        base_type = is_unsigned ? types->getBase(1, TYPE_UINT) : types->getBase(1, TYPE_INT);
    }
    else if (type_name == "short" || type_name == "short int") {
        base_type = types->getBase(2, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "int") {
        base_type = types->getBase(4, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "long" || type_name == "long int") {
        // Windows: long is 4 bytes (LLP64 model)
        base_type = types->getBase(4, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "long long" || type_name == "long long int") {
        base_type = types->getBase(8, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "__int8") {
        base_type = types->getBase(1, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "__int16") {
        base_type = types->getBase(2, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "__int32") {
        base_type = types->getBase(4, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "__int64") {
        base_type = types->getBase(8, is_unsigned ? TYPE_UINT : TYPE_INT);
    }
    else if (type_name == "float") {
        base_type = types->getBase(4, TYPE_FLOAT);
    }
    else if (type_name == "double") {
        base_type = types->getBase(8, TYPE_FLOAT);
    }
    else if (type_name == "long double") {
        // x86/x64: long double is typically 8 or 10 bytes, we use 8 for compatibility
        base_type = types->getBase(8, TYPE_FLOAT);
    }
    else if (type_name == "wchar_t") {
        // Windows: wchar_t is 2 bytes (UTF-16)
        base_type = types->getBase(2, TYPE_INT);
    }
    else if (type_name == "bool" || type_name == "_Bool") {
        base_type = types->getBase(1, TYPE_BOOL);
    }
    else {
        // Not a primitive - try to find by name in TypeFactory
        // This handles already-registered typedefs and structures
        base_type = types->findByName(type_name);

        // Also try with "struct " prefix removed if present
        if (!base_type && type_name.substr(0, 7) == "struct ") {
            base_type = types->findByName(type_name.substr(7));
        }

        // Try with "union " prefix removed
        if (!base_type && type_name.substr(0, 6) == "union ") {
            base_type = types->findByName(type_name.substr(6));
        }

        // Try with "enum " prefix removed
        if (!base_type && type_name.substr(0, 5) == "enum ") {
            base_type = types->findByName(type_name.substr(5));
        }
    }

    // If base type not found, return nullptr
    if (!base_type) {
        return nullptr;
    }

    // =========================================================================
    // Step 3: Wrap with pointer types if needed
    // =========================================================================
    Datatype* result = base_type;
    for (int i = 0; i < pointer_depth; i++) {
        result = types->getTypePointer(ptr_size, result, 0);
        if (!result) return nullptr;
    }

    return result;
}

// Simple JSON parser for GDT types.json format
// Extracts complete_structures and typedef_aliases from the JSON file
GdtData parse_gdt_json(const std::string& json_path) {
    GdtData result;
    
    std::ifstream file(json_path);
    if (!file.is_open()) {
        std::cerr << "[fission_decomp] Failed to open GDT JSON file: " << json_path << std::endl;
        return result;
    }
    
    std::stringstream buffer;
    buffer << file.rdbuf();
    std::string json = buffer.str();
    
    // Find "complete_structures" array
    size_t pos = json.find("\"complete_structures\"");
    if (pos == std::string::npos) {
        std::cerr << "[fission_decomp] 'complete_structures' not found in JSON" << std::endl;
        return result;
    }
    
    // Find the array start
    pos = json.find('[', pos);
    if (pos == std::string::npos) {
         std::cerr << "[fission_decomp] Array start '[' not found after 'complete_structures'" << std::endl;
         return result;
    }
    
    // Parse structures (simplified nested object parsing)
    size_t depth = 1;
    size_t start = pos + 1;
    
    while (pos < json.size() && depth > 0) {
        pos++;
        if (json[pos] == '[') depth++;
        else if (json[pos] == ']') depth--;
        else if (json[pos] == '{' && depth == 1) {
            // Start of a structure object
            size_t struct_start = pos;
            int struct_depth = 1;
            while (pos < json.size() && struct_depth > 0) {
                pos++;
                if (json[pos] == '{') struct_depth++;
                else if (json[pos] == '}') struct_depth--;
            }
            
            // Extract this structure's JSON
            std::string struct_json = json.substr(struct_start, pos - struct_start + 1);
            std::string meta_json = struct_json; // Copy for masking fields
            
            GdtStruct s;
            
            // Parse fields array within this struct
            size_t fields_pos = struct_json.find("\"fields\"");
            if (fields_pos != std::string::npos) {
                size_t arr_start = struct_json.find('[', fields_pos);
                if (arr_start != std::string::npos) {
                    int arr_depth = 1;
                    size_t fp = arr_start + 1;
                    while (fp < struct_json.size() && arr_depth > 0) {
                        if (struct_json[fp] == '{') {
                            size_t field_start = fp;
                            int fd = 1;
                            fp++;
                            while (fp < struct_json.size() && fd > 0) {
                                if (struct_json[fp] == '{') fd++;
                                else if (struct_json[fp] == '}') fd--;
                                fp++;
                            }
                            std::string field_json = struct_json.substr(field_start, fp - field_start);
                            
                            GdtField f;
                            f.name = extract_json_string(field_json, "name");
                            f.offset = (int)extract_json_int(field_json, "offset");
                            f.size = (int)extract_json_int(field_json, "size");
                            if (!f.name.empty() && f.size > 0) {
                                s.fields.push_back(f);
                            }
                        }
                        if (struct_json[fp] == '[') arr_depth++;
                        else if (struct_json[fp] == ']') arr_depth--;
                        fp++;
                    }
                    
                    // Mask fields array in meta_json to avoid finding field properties as struct properties
                    if (fp <= meta_json.size()) {
                        for (size_t i = fields_pos; i < fp; i++) {
                            meta_json[i] = ' ';
                        }
                    }
                }
            }
            
            s.name = extract_json_string(meta_json, "name");
            s.size = (int)extract_json_int(meta_json, "size");
            s.alignment = (int)extract_json_int(meta_json, "alignment");
            
            if (!s.name.empty() && s.size > 0) {
                result.structs.push_back(s);
            }
        }
    }

    // Parse typedefs
    pos = json.find("\"typedef_aliases\"");
    if (pos != std::string::npos) {
        pos = json.find('[', pos);
        if (pos != std::string::npos) {
            size_t depth = 1;
            pos++;
            while (pos < json.size() && depth > 0) {
                if (json[pos] == '[') depth++;
                else if (json[pos] == ']') depth--;
                else if (json[pos] == '{' && depth == 1) {
                    size_t start = pos;
                    int d = 1;
                    pos++;
                    while (pos < json.size() && d > 0) {
                        if (json[pos] == '{') d++;
                        else if (json[pos] == '}') d--;
                        pos++;
                    }
                    std::string td_json = json.substr(start, pos - start);
                    GdtTypedef td;
                    td.alias = extract_json_string(td_json, "alias");
                    td.base = extract_json_string(td_json, "base");
                    if (!td.alias.empty() && !td.base.empty()) {
                        result.typedefs.push_back(td);
                    }
                    pos--;
                }
                pos++;
            }
        }
    }

    // Parse enum values
    pos = json.find("\"enum_values\"");
    if (pos != std::string::npos) {
        pos = json.find('[', pos);
        if (pos != std::string::npos) {
            size_t depth = 1;
            pos++;
            while (pos < json.size() && depth > 0) {
                if (json[pos] == '[') depth++;
                else if (json[pos] == ']') depth--;
                else if (json[pos] == '{' && depth == 1) {
                    size_t start = pos;
                    int d = 1;
                    pos++;
                    while (pos < json.size() && d > 0) {
                        if (json[pos] == '{') d++;
                        else if (json[pos] == '}') d--;
                        pos++;
                    }
                    std::string enum_json = json.substr(start, pos - start);
                    GdtEnum e;
                    e.name = extract_json_string(enum_json, "name");
                    e.value = extract_json_int(enum_json, "value");
                    if (!e.name.empty() && e.value > 0) {
                        result.enums.push_back(e);
                    }
                    pos--;
                }
                pos++;
            }
        }
    }
    
    // std::cerr << "[fission_decomp] Parsed " << result.structs.size() << " structures and " << result.typedefs.size() << " typedefs from " << json_path << std::endl;
    
    return result;
}

// Primitive type mapping: C type names -> Ghidra (size, metatype)
static std::map<std::string, std::pair<int, type_metatype>> PRIMITIVE_MAP = {
    // Signed integers
    {"char", {1, TYPE_INT}},
    {"signed char", {1, TYPE_INT}},
    {"short", {2, TYPE_INT}},
    {"short int", {2, TYPE_INT}},
    {"signed short", {2, TYPE_INT}},
    {"int", {4, TYPE_INT}},
    {"signed int", {4, TYPE_INT}},
    {"long", {4, TYPE_INT}},          // Windows: long is 4 bytes
    {"long int", {4, TYPE_INT}},
    {"signed long", {4, TYPE_INT}},
    {"long long", {8, TYPE_INT}},
    {"long long int", {8, TYPE_INT}},
    {"signed long long", {8, TYPE_INT}},
    {"__int64", {8, TYPE_INT}},
    {"__int32", {4, TYPE_INT}},
    {"__int16", {2, TYPE_INT}},
    {"__int8", {1, TYPE_INT}},
    
    // Unsigned integers
    {"unsigned char", {1, TYPE_UINT}},
    {"unsigned short", {2, TYPE_UINT}},
    {"unsigned short int", {2, TYPE_UINT}},
    {"unsigned", {4, TYPE_UINT}},
    {"unsigned int", {4, TYPE_UINT}},
    {"unsigned long", {4, TYPE_UINT}},
    {"unsigned long int", {4, TYPE_UINT}},
    {"unsigned long long", {8, TYPE_UINT}},
    {"unsigned long long int", {8, TYPE_UINT}},
    {"unsigned __int64", {8, TYPE_UINT}},
    {"unsigned __int32", {4, TYPE_UINT}},
    {"unsigned __int16", {2, TYPE_UINT}},
    {"unsigned __int8", {1, TYPE_UINT}},
    
    // Special types
    {"void", {0, TYPE_VOID}},
    {"bool", {1, TYPE_BOOL}},
    {"_Bool", {1, TYPE_BOOL}},
    {"wchar_t", {2, TYPE_INT}},
    {"float", {4, TYPE_FLOAT}},
    {"double", {8, TYPE_FLOAT}},
    {"long double", {8, TYPE_FLOAT}},  // Simplified: treat as 8 bytes
};

// Resolve GDT type name to Ghidra Datatype
// Handles: exact match, pointer syntax (char *), primitive mapping (int -> int4)
Datatype* resolve_gdt_type(TypeFactory* types, const std::string& name, bool is_64bit) {
    if (name.empty() || !types) return nullptr;
    
    // 1. Try exact match first
    Datatype* dt = types->findByName(name);
    if (dt) return dt;
    
    // 2. Handle pointer syntax (e.g., "char *", "void **", "HANDLE *")
    std::string trimmed = name;
    int pointer_depth = 0;
    
    // Trim trailing whitespace
    while (!trimmed.empty() && (trimmed.back() == ' ' || trimmed.back() == '\t')) {
        trimmed.pop_back();
    }
    
    // Count and strip trailing '*' characters
    while (!trimmed.empty() && trimmed.back() == '*') {
        pointer_depth++;
        trimmed.pop_back();
        // Trim spaces between * and base type
        while (!trimmed.empty() && (trimmed.back() == ' ' || trimmed.back() == '\t')) {
            trimmed.pop_back();
        }
    }
    
    // If we have pointers, recursively resolve base type
    if (pointer_depth > 0) {
        Datatype* base_type = resolve_gdt_type(types, trimmed, is_64bit);
        if (base_type) {
            int ptr_size = is_64bit ? 8 : 4;
            Datatype* result = base_type;
            for (int i = 0; i < pointer_depth; i++) {
                result = types->getTypePointer(ptr_size, result, 1);
            }
            return result;
        }
        return nullptr;  // Base type not found
    }
    
    // 3. Try primitive type mapping
    auto it = PRIMITIVE_MAP.find(trimmed);
    if (it != PRIMITIVE_MAP.end()) {
        if (it->second.second == TYPE_VOID) {
            return types->getTypeVoid();
        }
        return types->getBase(it->second.first, it->second.second);
    }
    
    // 4. Try with "struct " prefix removed
    if (trimmed.length() > 7 && trimmed.substr(0, 7) == "struct ") {
        dt = types->findByName(trimmed.substr(7));
        if (dt) return dt;
    }
    
    // 5. Try with "union " prefix removed
    if (trimmed.length() > 6 && trimmed.substr(0, 6) == "union ") {
        dt = types->findByName(trimmed.substr(6));
        if (dt) return dt;
    }
    
    // 6. Try with "enum " prefix removed
    if (trimmed.length() > 5 && trimmed.substr(0, 5) == "enum ") {
        dt = types->findByName(trimmed.substr(5));
        if (dt) return dt;
    }
    
    // 7. Windows pointer typedef heuristics: LPXXX -> XXX*, PXXX -> XXX*, LPCXXX -> XXX*
    // This handles common Windows typedefs like LPHANDLE, LPDWORD, LPVOID, PHANDLE, etc.
    int ptr_size = is_64bit ? 8 : 4;
    
    // LPC prefix (Long Pointer to Const): LPCSTR, LPCWSTR, etc.
    if (trimmed.length() > 3 && trimmed.substr(0, 3) == "LPC") {
        std::string base_name = trimmed.substr(3);  // Remove "LPC"
        Datatype* base_type = resolve_gdt_type(types, base_name, is_64bit);
        if (base_type) {
            return types->getTypePointer(ptr_size, base_type, 1);
        }
    }
    
    // LP prefix (Long Pointer): LPHANDLE, LPDWORD, LPVOID, etc.
    if (trimmed.length() > 2 && trimmed.substr(0, 2) == "LP") {
        std::string base_name = trimmed.substr(2);  // Remove "LP"
        Datatype* base_type = resolve_gdt_type(types, base_name, is_64bit);
        if (base_type) {
            return types->getTypePointer(ptr_size, base_type, 1);
        }
    }
    
    // P prefix (Pointer): PHANDLE, PDWORD, PVOID, etc. (but not PP for double pointer)
    if (trimmed.length() > 1 && trimmed[0] == 'P' && std::isupper(trimmed[1]) && trimmed[1] != 'P') {
        std::string base_name = trimmed.substr(1);  // Remove "P"
        Datatype* base_type = resolve_gdt_type(types, base_name, is_64bit);
        if (base_type) {
            return types->getTypePointer(ptr_size, base_type, 1);
        }
    }
    
    return nullptr;
}

// Load GDT types into TypeFactory
// Uses resolve_gdt_type for proper C type -> Ghidra type conversion
void load_gdt_types(CliArchitecture* arch, const std::string& gdt_json_path, bool is_64bit = true) {
    if (!arch || gdt_json_path.empty()) return;

    TypeFactory* types = arch->types;
    if (!types) return;

    // Pointer size depends on architecture
    int ptr_size = is_64bit ? 8 : 4;

    GdtData gdt_data = parse_gdt_json(gdt_json_path);
    if (gdt_data.structs.empty() && gdt_data.typedefs.empty()) {
        std::cerr << "[fission_decomp] No GDT types loaded from: " << gdt_json_path << std::endl;
        return;
    }

    int loaded = 0;
    for (const auto& gdt : gdt_data.structs) {
        try {
            // Check if type already exists
            Datatype* existing = types->findByName(gdt.name);
            if (existing != nullptr) continue;

            // Create empty structure
            TypeStruct* ts = types->getTypeStruct(gdt.name);
            if (!ts) continue;

            // Build field list
            std::vector<TypeField> fields;
            int field_id = 0;
            for (const auto& f : gdt.fields) {
                // Get appropriate base type for field size
                Datatype* field_type = nullptr;
                if (f.size == 1) {
                    field_type = types->getBase(1, TYPE_UINT);
                } else if (f.size == 2) {
                    field_type = types->getBase(2, TYPE_UINT);
                } else if (f.size == 4) {
                    field_type = types->getBase(4, TYPE_UINT);
                } else if (f.size == 8) {
                    field_type = types->getBase(8, TYPE_UINT);
                } else {
                    // For other sizes, create array of bytes
                    Datatype* byte_type = types->getBase(1, TYPE_UINT);
                    field_type = types->getTypeArray(f.size, byte_type);
                }

                if (field_type) {
                    TypeField tf(field_id, f.offset, f.name, field_type);
                    fields.push_back(tf);
                    field_id++;
                }
            }

            // Set fields on the structure
            if (!fields.empty()) {
                types->setFields(fields, ts, gdt.size, gdt.alignment, 0);
                loaded++;
            }
        } catch (...) {
            // Ignore type creation errors
        }
    }

    // =========================================================================
    // Register typedefs using resolve_gdt_type (Multi-pass for dependency resolution)
    // =========================================================================
    // Pass 1-N: Keep trying until no more progress is made
    // This handles typedef chains like: LPSTR -> char* -> char
    // =========================================================================
    int loaded_typedefs = 0;
    int last_pass_count = -1;
    int pass = 0;
    const int MAX_PASSES = 10; // Increased from 5 for deeper chains
    std::vector<bool> registered_td(gdt_data.typedefs.size(), false);

    while (pass < MAX_PASSES && (size_t)loaded_typedefs < gdt_data.typedefs.size() && loaded_typedefs != last_pass_count) {
        last_pass_count = loaded_typedefs;
        for (size_t i = 0; i < gdt_data.typedefs.size(); i++) {
            if (registered_td[i]) continue;

            try {
                const std::string& base_name = gdt_data.typedefs[i].base;
                const std::string& alias_name = gdt_data.typedefs[i].alias;

                // Use resolve_gdt_type to handle C primitives and pointers
                Datatype* base_type = resolve_gdt_type(types, base_name, is_64bit);

                if (base_type) {
                    // Check if alias already exists
                    if (types->findByName(alias_name)) {
                         registered_td[i] = true;
                         loaded_typedefs++;
                         continue;
                    }

                    // Create the typedef
                    types->getTypedef(base_type, alias_name, 0, 0);
                    registered_td[i] = true;
                    loaded_typedefs++;
                }
            } catch (...) {}
        }
        pass++;
    }
    
    std::cerr << "[fission_decomp] Loaded " << loaded << " GDT structures and " 
              << loaded_typedefs << " typedefs from " << gdt_json_path << std::endl;
}

// Load GDT enum values for constant name substitution
// Returns a map of value -> name for post-processing
std::map<uint64_t, std::string> load_gdt_enums(const std::string& gdt_json_path) {
    std::map<uint64_t, std::string> result;
    
    if (gdt_json_path.empty()) return result;
    
    GdtData gdt_data = parse_gdt_json(gdt_json_path);
    
    for (const auto& e : gdt_data.enums) {
        // Only add if value is not already mapped (first wins)
        if (result.find(e.value) == result.end()) {
            result[e.value] = e.name;
        }
    }
    
    if (!result.empty()) {
        std::cerr << "[fission_decomp] Loaded " << result.size() << " enum constants for substitution" << std::endl;
    }
    
    return result;
}

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
    int registered = 0;

    try {
        // Get base types for typedef creation
        Datatype* voidType = types->getTypeVoid();
        Datatype* charType = types->getBase(1, TYPE_INT); // char as 1-byte signed int
        Datatype* ucharType = types->getBase(1, TYPE_UINT);
        Datatype* wcharType = types->getBase(2, TYPE_INT); // wchar_t is 2 bytes on Windows
        Datatype* int16Type = types->getBase(2, TYPE_INT);
        Datatype* uint16Type = types->getBase(2, TYPE_UINT);
        Datatype* int32Type = types->getBase(4, TYPE_INT);
        Datatype* uint32Type = types->getBase(4, TYPE_UINT);
        Datatype* int64Type = types->getBase(8, TYPE_INT);
        Datatype* uint64Type = types->getBase(8, TYPE_UINT);
        Datatype* intptrType = types->getBase(ptrSize, TYPE_INT);
        Datatype* uintptrType = types->getBase(ptrSize, TYPE_UINT);

        // Pointer types
        Datatype* voidPtrType = types->getTypePointer(ptrSize, voidType, 0);
        Datatype* charPtrType = types->getTypePointer(ptrSize, charType, 0);
        Datatype* wcharPtrType = types->getTypePointer(ptrSize, wcharType, 0);
        Datatype* constCharPtrType = charPtrType;  // Ghidra doesn't distinguish const
        Datatype* constWcharPtrType = wcharPtrType;
        Datatype* voidPtrPtrType = types->getTypePointer(ptrSize, voidPtrType, 0);

        // =========================================================================
        // Register Windows typedefs using getTypedef
        // These are the most common Windows API types
        // =========================================================================

        // --- Integer types ---
        if (!types->findByName("BYTE")) { types->getTypedef(ucharType, "BYTE", 0, 0); registered++; }
        if (!types->findByName("WORD")) { types->getTypedef(uint16Type, "WORD", 0, 0); registered++; }
        if (!types->findByName("DWORD")) { types->getTypedef(uint32Type, "DWORD", 0, 0); registered++; }
        if (!types->findByName("QWORD")) { types->getTypedef(uint64Type, "QWORD", 0, 0); registered++; }

        if (!types->findByName("CHAR")) { types->getTypedef(charType, "CHAR", 0, 0); registered++; }
        if (!types->findByName("WCHAR")) { types->getTypedef(wcharType, "WCHAR", 0, 0); registered++; }
        if (!types->findByName("SHORT")) { types->getTypedef(int16Type, "SHORT", 0, 0); registered++; }
        if (!types->findByName("USHORT")) { types->getTypedef(uint16Type, "USHORT", 0, 0); registered++; }
        if (!types->findByName("INT")) { types->getTypedef(int32Type, "INT", 0, 0); registered++; }
        if (!types->findByName("UINT")) { types->getTypedef(uint32Type, "UINT", 0, 0); registered++; }
        if (!types->findByName("LONG")) { types->getTypedef(int32Type, "LONG", 0, 0); registered++; }
        if (!types->findByName("ULONG")) { types->getTypedef(uint32Type, "ULONG", 0, 0); registered++; }
        if (!types->findByName("LONGLONG")) { types->getTypedef(int64Type, "LONGLONG", 0, 0); registered++; }
        if (!types->findByName("ULONGLONG")) { types->getTypedef(uint64Type, "ULONGLONG", 0, 0); registered++; }

        if (!types->findByName("BOOL")) { types->getTypedef(int32Type, "BOOL", 0, 0); registered++; }
        if (!types->findByName("BOOLEAN")) { types->getTypedef(ucharType, "BOOLEAN", 0, 0); registered++; }

        // --- Size types (architecture-dependent) ---
        if (!types->findByName("SIZE_T")) { types->getTypedef(uintptrType, "SIZE_T", 0, 0); registered++; }
        if (!types->findByName("SSIZE_T")) { types->getTypedef(intptrType, "SSIZE_T", 0, 0); registered++; }
        if (!types->findByName("ULONG_PTR")) { types->getTypedef(uintptrType, "ULONG_PTR", 0, 0); registered++; }
        if (!types->findByName("LONG_PTR")) { types->getTypedef(intptrType, "LONG_PTR", 0, 0); registered++; }
        if (!types->findByName("DWORD_PTR")) { types->getTypedef(uintptrType, "DWORD_PTR", 0, 0); registered++; }
        if (!types->findByName("INT_PTR")) { types->getTypedef(intptrType, "INT_PTR", 0, 0); registered++; }
        if (!types->findByName("UINT_PTR")) { types->getTypedef(uintptrType, "UINT_PTR", 0, 0); registered++; }

        // --- Handle types (all void*) ---
        if (!types->findByName("HANDLE")) { types->getTypedef(voidPtrType, "HANDLE", 0, 0); registered++; }
        if (!types->findByName("HMODULE")) { types->getTypedef(voidPtrType, "HMODULE", 0, 0); registered++; }
        if (!types->findByName("HINSTANCE")) { types->getTypedef(voidPtrType, "HINSTANCE", 0, 0); registered++; }
        if (!types->findByName("HWND")) { types->getTypedef(voidPtrType, "HWND", 0, 0); registered++; }
        if (!types->findByName("HDC")) { types->getTypedef(voidPtrType, "HDC", 0, 0); registered++; }
        if (!types->findByName("HBRUSH")) { types->getTypedef(voidPtrType, "HBRUSH", 0, 0); registered++; }
        if (!types->findByName("HFONT")) { types->getTypedef(voidPtrType, "HFONT", 0, 0); registered++; }
        if (!types->findByName("HICON")) { types->getTypedef(voidPtrType, "HICON", 0, 0); registered++; }
        if (!types->findByName("HCURSOR")) { types->getTypedef(voidPtrType, "HCURSOR", 0, 0); registered++; }
        if (!types->findByName("HMENU")) { types->getTypedef(voidPtrType, "HMENU", 0, 0); registered++; }
        if (!types->findByName("HBITMAP")) { types->getTypedef(voidPtrType, "HBITMAP", 0, 0); registered++; }
        if (!types->findByName("HGLOBAL")) { types->getTypedef(voidPtrType, "HGLOBAL", 0, 0); registered++; }
        if (!types->findByName("HLOCAL")) { types->getTypedef(voidPtrType, "HLOCAL", 0, 0); registered++; }
        if (!types->findByName("HKEY")) { types->getTypedef(voidPtrType, "HKEY", 0, 0); registered++; }
        if (!types->findByName("HFILE")) { types->getTypedef(int32Type, "HFILE", 0, 0); registered++; }
        if (!types->findByName("HRESULT")) { types->getTypedef(int32Type, "HRESULT", 0, 0); registered++; }
        if (!types->findByName("NTSTATUS")) { types->getTypedef(int32Type, "NTSTATUS", 0, 0); registered++; }

        // --- Pointer types ---
        if (!types->findByName("PVOID")) { types->getTypedef(voidPtrType, "PVOID", 0, 0); registered++; }
        if (!types->findByName("LPVOID")) { types->getTypedef(voidPtrType, "LPVOID", 0, 0); registered++; }
        if (!types->findByName("LPCVOID")) { types->getTypedef(voidPtrType, "LPCVOID", 0, 0); registered++; }
        if (!types->findByName("PPVOID")) { types->getTypedef(voidPtrPtrType, "PPVOID", 0, 0); registered++; }

        if (!types->findByName("LPSTR")) { types->getTypedef(charPtrType, "LPSTR", 0, 0); registered++; }
        if (!types->findByName("LPCSTR")) { types->getTypedef(constCharPtrType, "LPCSTR", 0, 0); registered++; }
        if (!types->findByName("PSTR")) { types->getTypedef(charPtrType, "PSTR", 0, 0); registered++; }
        if (!types->findByName("PCSTR")) { types->getTypedef(constCharPtrType, "PCSTR", 0, 0); registered++; }

        if (!types->findByName("LPWSTR")) { types->getTypedef(wcharPtrType, "LPWSTR", 0, 0); registered++; }
        if (!types->findByName("LPCWSTR")) { types->getTypedef(constWcharPtrType, "LPCWSTR", 0, 0); registered++; }
        if (!types->findByName("PWSTR")) { types->getTypedef(wcharPtrType, "PWSTR", 0, 0); registered++; }
        if (!types->findByName("PCWSTR")) { types->getTypedef(constWcharPtrType, "PCWSTR", 0, 0); registered++; }

        // TCHAR variants (assume Unicode)
        if (!types->findByName("TCHAR")) { types->getTypedef(wcharType, "TCHAR", 0, 0); registered++; }
        if (!types->findByName("LPTSTR")) { types->getTypedef(wcharPtrType, "LPTSTR", 0, 0); registered++; }
        if (!types->findByName("LPCTSTR")) { types->getTypedef(constWcharPtrType, "LPCTSTR", 0, 0); registered++; }

        // Pointer to integer types
        if (!types->findByName("PBYTE")) { types->getTypedef(types->getTypePointer(ptrSize, ucharType, 0), "PBYTE", 0, 0); registered++; }
        if (!types->findByName("LPBYTE")) { types->getTypedef(types->getTypePointer(ptrSize, ucharType, 0), "LPBYTE", 0, 0); registered++; }
        if (!types->findByName("PWORD")) { types->getTypedef(types->getTypePointer(ptrSize, uint16Type, 0), "PWORD", 0, 0); registered++; }
        if (!types->findByName("LPWORD")) { types->getTypedef(types->getTypePointer(ptrSize, uint16Type, 0), "LPWORD", 0, 0); registered++; }
        if (!types->findByName("PDWORD")) { types->getTypedef(types->getTypePointer(ptrSize, uint32Type, 0), "PDWORD", 0, 0); registered++; }
        if (!types->findByName("LPDWORD")) { types->getTypedef(types->getTypePointer(ptrSize, uint32Type, 0), "LPDWORD", 0, 0); registered++; }
        if (!types->findByName("PLONG")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "PLONG", 0, 0); registered++; }
        if (!types->findByName("LPLONG")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "LPLONG", 0, 0); registered++; }
        if (!types->findByName("PBOOL")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "PBOOL", 0, 0); registered++; }
        if (!types->findByName("LPBOOL")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "LPBOOL", 0, 0); registered++; }
        if (!types->findByName("PINT")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "PINT", 0, 0); registered++; }
        if (!types->findByName("LPINT")) { types->getTypedef(types->getTypePointer(ptrSize, int32Type, 0), "LPINT", 0, 0); registered++; }
        if (!types->findByName("PUINT")) { types->getTypedef(types->getTypePointer(ptrSize, uint32Type, 0), "PUINT", 0, 0); registered++; }
        if (!types->findByName("PSIZE_T")) { types->getTypedef(types->getTypePointer(ptrSize, uintptrType, 0), "PSIZE_T", 0, 0); registered++; }

        // --- Special Win32 types ---
        if (!types->findByName("WPARAM")) { types->getTypedef(uintptrType, "WPARAM", 0, 0); registered++; }
        if (!types->findByName("LPARAM")) { types->getTypedef(intptrType, "LPARAM", 0, 0); registered++; }
        if (!types->findByName("LRESULT")) { types->getTypedef(intptrType, "LRESULT", 0, 0); registered++; }
        if (!types->findByName("ATOM")) { types->getTypedef(uint16Type, "ATOM", 0, 0); registered++; }
        if (!types->findByName("COLORREF")) { types->getTypedef(uint32Type, "COLORREF", 0, 0); registered++; }

        // --- Security types ---
        if (!types->findByName("SECURITY_STATUS")) { types->getTypedef(int32Type, "SECURITY_STATUS", 0, 0); registered++; }

        std::cerr << "[fission_decomp] Registered " << registered << " Windows typedefs (" << (is_64bit ? "64" : "32") << "-bit)" << std::endl;
    } catch (...) {
        // Type registration is best-effort
    }
}

// Register known global symbols (security cookie, etc.)
void register_known_globals(CliArchitecture* arch, uint64_t image_base, bool is_64bit) {
    if (!arch || !arch->symboltab) return;
    
    Scope* global_scope = arch->symboltab->getGlobalScope();
    if (!global_scope) return;
    
    // Known MSVC global variable patterns
    // These are common addresses relative to typical PE layout
    // Note: Actual offsets depend on the binary, but we can add named symbols
    // for common patterns that appear in the decompiled output
    
    try {
        // The __security_cookie is typically at a known offset in .data section
        // For PyInstaller binaries like the user's, it's at base + 0x40040 area
        // We'll register some common patterns as named data symbols
        
        // For now, we'll inject known globals via the IAT symbols mechanism
        // since Ghidra's standalone mode has limited data symbol support
        
        std::cerr << "[fission_decomp] Known globals registration skipped (using IAT mechanism)" << std::endl;
    } catch (...) {
        // Global registration is best-effort
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
                // Load GDT types if provided (64-bit)
                if (!gdt_json.empty()) {
                    load_gdt_types(state.arch_64bit, gdt_json, true);
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
            inject_iat_symbols(state.arch_64bit, iat_symbols);
            // Store for post-processing
            state.iat_symbols = iat_symbols;
            
            // Load enum values for constant substitution (from 64-bit GDT)
            if (!gdt_json.empty() && state.enum_values.empty()) {
                state.enum_values = load_gdt_enums(gdt_json);
            }

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
                // Load GDT types if provided (use 32-bit GDT for 32-bit arch if available)
                if (!gdt_json.empty()) {
                    // Try to find 32-bit version by replacing "64" with "32" in path
                    std::string gdt_json_32 = gdt_json;
                    size_t pos = gdt_json_32.find("64");
                    if (pos != std::string::npos) {
                        gdt_json_32.replace(pos, 2, "32");
                    }
                    load_gdt_types(state.arch_32bit, gdt_json_32, false);
                }
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
