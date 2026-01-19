#include "fission/types/GdtBinaryParser.h"
#include "fission/util/BinaryReader.h"
#include <iostream>
#include "fission/utils/logger.h"
#include <cstring>
#include <algorithm>
#include <zlib.h>
#include <sstream>
#include <cctype>
#include <set>

namespace fission {
namespace types {

using fission::util::BinaryReader;

// Ghidra DB4 format constants
static const char DB4_MAGIC[] = "/01,4),*";
static const uint32_t BUFFER_SIZE = 0x4000;

// Known table names in Ghidra DB
static const std::string FUNCTION_DEF_TABLE = "Function Definitions";
static const std::string PARAMETER_TABLE = "Function Parameters";
static const std::string CALLING_CONV_TABLE = "Calling Conventions";
static const std::string CATEGORY_TABLE = "Categories";
static const std::string TYPEDEF_TABLE = "Typedefs";
static const std::string POINTER_TABLE = "Pointers";
static const std::string COMPOSITE_TABLE = "Composite Data Types";

// Table schema found in DB:
// Function Definitions: Data Type ID;Name;Comment;Category ID;Return Type ID;Flags;Call Conv ID;Source Archive ID;Source Data Type ID;Source Sync Time;Last Change Time
// Function Parameters: Parameter ID;Parent ID;Data Type ID;Name;Comment;Ordinal;Data Type Length

GdtBinaryParser::GdtBinaryParser() : loaded(false), file_type(0), content_length(0) {}
GdtBinaryParser::~GdtBinaryParser() {}

std::string GdtBinaryParser::read_java_utf(std::ifstream& file) {
    return BinaryReader::read_java_utf(file);
}

int32_t GdtBinaryParser::read_java_int(std::ifstream& file) {
    return BinaryReader::read_java_int(file);
}

int64_t GdtBinaryParser::read_java_long(std::ifstream& file) {
    return BinaryReader::read_java_long(file);
}

bool GdtBinaryParser::parse_header(std::ifstream& file) {
    file.seekg(6, std::ios::beg);
    
    int64_t magic = read_java_long(file);
    if ((uint64_t)magic != MAGIC_NUMBER) {
        fission::utils::log_stream() << "[GdtBinaryParser] Invalid magic" << std::endl;
        return false;
    }
    
    int32_t version = read_java_int(file);
    if (version != FORMAT_VERSION) {
        fission::utils::log_stream() << "[GdtBinaryParser] Unsupported version: " << version << std::endl;
        return false;
    }
    
    item_name = read_java_utf(file);
    content_type = read_java_utf(file);
    file_type = read_java_int(file);
    content_length = read_java_long(file);
    
    fission::utils::log_stream() << "[GdtBinaryParser] Header: " << item_name << ", type=" << content_type << std::endl;
    return true;
}

bool GdtBinaryParser::decompress_content(std::ifstream& file, std::vector<uint8_t>& out) {
    uint8_t byte;
    bool found = false;
    while (file.read(reinterpret_cast<char*>(&byte), 1)) {
        if (byte == 'P') {
            uint8_t next[3];
            file.read(reinterpret_cast<char*>(next), 3);
            if (next[0] == 'K' && next[1] == 0x03 && next[2] == 0x04) {
                file.seekg(-4, std::ios::cur);
                found = true;
                break;
            }
            file.seekg(-3, std::ios::cur);
        }
    }
    
    if (!found) return false;
    
    std::streampos zip_start = file.tellg();
    file.seekg(0, std::ios::end);
    std::streampos file_end = file.tellg();
    size_t zip_size = file_end - zip_start;
    
    std::vector<uint8_t> zip_data(zip_size);
    file.seekg(zip_start);
    file.read(reinterpret_cast<char*>(zip_data.data()), zip_size);
    
    if (zip_size < 30) return false;
    
    const uint8_t* p = zip_data.data();
    uint16_t compression_method = p[8] | (p[9] << 8);
    uint16_t filename_len = p[26] | (p[27] << 8);
    uint16_t extra_len = p[28] | (p[29] << 8);
    
    size_t data_offset = 30 + filename_len + extra_len;
    if (data_offset >= zip_size) return false;
    
    const uint8_t* compressed_data = p + data_offset;
    size_t available = zip_size - data_offset;
    
    if (compression_method == 8) {
        // Allocate large buffer for Ghidra DB (typically ~30MB)
        out.resize(50 * 1024 * 1024);
        
        z_stream strm = {};
        strm.next_in = (Bytef*)compressed_data;
        strm.avail_in = available;
        strm.next_out = out.data();
        strm.avail_out = out.size();
        
        if (inflateInit2(&strm, -MAX_WBITS) != Z_OK) return false;
        
        int ret = inflate(&strm, Z_FINISH);
        size_t decompressed = strm.total_out;
        inflateEnd(&strm);
        
        if (ret != Z_STREAM_END && ret != Z_OK) return false;
        out.resize(decompressed);
    } else {
        return false;
    }
    
    fission::utils::log_stream() << "[GdtBinaryParser] Decompressed: " << out.size() << " bytes" << std::endl;
    return true;
}

// Parse a length-prefixed string from DB data
static bool extract_length_prefixed_string(const uint8_t* data, size_t max_len, 
                                           std::string& out, size_t& bytes_consumed) {
    if (max_len < 1) return false;
    
    size_t len = data[0];
    if (len == 0 || len > max_len - 1 || len > 255) return false;
    
    out.assign(reinterpret_cast<const char*>(data + 1), len);
    bytes_consumed = 1 + len;
    return true;
}

// Check if string is a valid identifier
static bool is_valid_identifier(const std::string& s) {
    if (s.empty() || s.length() > 256) return false;
    if (!std::isalpha(s[0]) && s[0] != '_') return false;
    for (char c : s) {
        if (!std::isalnum(c) && c != '_') return false;
    }
    return true;
}

// Parse Function Definitions table from DB content
void GdtBinaryParser::parse_function_definitions(const std::vector<uint8_t>& data) {
    // Find Function Definitions table marker
    auto it = std::search(data.begin(), data.end(),
                          FUNCTION_DEF_TABLE.begin(), FUNCTION_DEF_TABLE.end());
    if (it == data.end()) {
        fission::utils::log_stream() << "[GdtBinaryParser] Function Definitions table not found" << std::endl;
        return;
    }
    
    size_t table_pos = std::distance(data.begin(), it);
    fission::utils::log_stream() << "[GdtBinaryParser] Function Definitions at: 0x" << std::hex << table_pos << std::dec << std::endl;
    
    // Extract names from the entire database
    std::set<std::string> unique_names;
    
    for (size_t i = 0; i < data.size() - 4; ++i) {
        // Look for 1-byte length-prefixed strings
        size_t len = data[i];
        if (len >= 3 && len <= 100 && i + 1 + len <= data.size()) {
            std::string s(reinterpret_cast<const char*>(&data[i + 1]), len);
            
            if (is_valid_identifier(s) && s[0] >= 'A' && s[0] <= 'Z') {
                unique_names.insert(s);
            }
        }
    }
    
    // Create function prototypes
    for (const auto& name : unique_names) {
        GdtFunctionProto proto;
        proto.name = name;
        proto.return_type = "undefined";
        
        // Infer calling convention from name patterns
        if (name.length() > 1) {
            char last = name[name.length() - 1];
            if (last == 'W' || last == 'A') {
                proto.calling_convention = "__stdcall";
            } else if (name.find("Rtl") == 0 || name.find("Nt") == 0 || 
                       name.find("Zw") == 0) {
                proto.calling_convention = "__stdcall";
            } else if (name[0] == '_') {
                proto.calling_convention = "__cdecl";
            }
        }
        
        functions[name] = proto;
    }
    
    fission::utils::log_stream() << "[GdtBinaryParser] Extracted " << functions.size() << " function names" << std::endl;
}

// Parse parameter types from DB
void GdtBinaryParser::parse_parameters(const std::vector<uint8_t>& data) {
    // Common Windows type names
    static const std::vector<std::pair<std::string, int>> windows_types = {
        {"HANDLE", 8}, {"DWORD", 4}, {"LPVOID", 8}, {"LPCSTR", 8}, {"LPCWSTR", 8},
        {"BOOL", 4}, {"INT", 4}, {"UINT", 4}, {"LPSTR", 8}, {"LPWSTR", 8},
        {"PVOID", 8}, {"ULONG", 4}, {"LONG", 4}, {"SIZE_T", 8}, {"LPARAM", 8},
        {"WPARAM", 8}, {"HINSTANCE", 8}, {"HWND", 8}, {"HDC", 8}, {"HMODULE", 8},
        {"HRESULT", 4}, {"NTSTATUS", 4}, {"LPDWORD", 8}, {"BYTE", 1}, {"WORD", 2},
        {"CHAR", 1}, {"WCHAR", 2}, {"SHORT", 2}, {"USHORT", 2}, {"UCHAR", 1},
        {"VOID", 0}, {"LPBYTE", 8}, {"LPCVOID", 8}, {"HKEY", 8}, {"HGLOBAL", 8},
        {"HLOCAL", 8}, {"HRSRC", 8}, {"FARPROC", 8}, {"SOCKET", 8}, {"ATOM", 2}
    };
    
    for (const auto& [type_name, size] : windows_types) {
        auto it = data.begin();
        while (it != data.end()) {
            it = std::search(it, data.end(), type_name.begin(), type_name.end());
            if (it != data.end()) {
                GdtDataType dt;
                dt.name = type_name;
                dt.category = "typedef";
                dt.size = size;
                dt.alignment = size > 0 ? std::min(size, 8) : 1;
                types[type_name] = dt;
                break; // Only need to find once
            }
        }
    }
    
    fission::utils::log_stream() << "[GdtBinaryParser] Found " << types.size() << " type definitions" << std::endl;
}

bool GdtBinaryParser::parse_db_content(const std::vector<uint8_t>& data) {
    // Check DB4 magic
    if (data.size() >= 8 && memcmp(data.data(), DB4_MAGIC, 8) == 0) {
        fission::utils::log_stream() << "[GdtBinaryParser] Valid DB4 format (buffer size: 0x" 
                  << std::hex << BUFFER_SIZE << std::dec << ")" << std::endl;
    }
    
    parse_function_definitions(data);
    parse_parameters(data);
    
    return !functions.empty();
}

bool GdtBinaryParser::load(const std::string& path) {
    filepath = path;
    loaded = false;
    functions.clear();
    types.clear();
    
    std::ifstream file(path, std::ios::binary);
    if (!file.is_open()) {
        fission::utils::log_stream() << "[GdtBinaryParser] Failed to open: " << path << std::endl;
        return false;
    }
    
    file.seekg(0, std::ios::end);
    size_t file_size = file.tellg();
    file.seekg(0);
    
    fission::utils::log_stream() << "[GdtBinaryParser] Loading " << path << " (" << file_size << " bytes)" << std::endl;
    
    if (!parse_header(file)) return false;
    
    std::vector<uint8_t> content;
    if (!decompress_content(file, content)) return false;
    
    if (!parse_db_content(content)) return false;
    
    loaded = !functions.empty();
    return loaded;
}

const GdtFunctionProto* GdtBinaryParser::find_function(const std::string& name) const {
    auto it = functions.find(name);
    if (it != functions.end()) return &it->second;
    
    // Try W/A variants
    if (name.length() > 1) {
        char last = name[name.length() - 1];
        if (last == 'W' || last == 'A') {
            std::string base = name.substr(0, name.length() - 1);
            auto it2 = functions.find(base);
            if (it2 != functions.end()) return &it2->second;
            
            std::string other = base + (last == 'W' ? 'A' : 'W');
            auto it3 = functions.find(other);
            if (it3 != functions.end()) return &it3->second;
        }
    }
    
    return nullptr;
}

const GdtDataType* GdtBinaryParser::find_type(const std::string& name) const {
    auto it = types.find(name);
    return (it != types.end()) ? &it->second : nullptr;
}

} // namespace types
} // namespace fission
