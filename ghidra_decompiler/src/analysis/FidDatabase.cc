#include "fission/analysis/FidDatabase.h"
#include <iostream>
#include <cstring>
#include <algorithm>

namespace fission {
namespace analysis {

// Known offsets in FIDBF format (from reverse engineering)
static const uint64_t HEADER_MAGIC_OFFSET = 0x0000;
static const uint64_t TABLE_INDEX_OFFSET = 0x4000;
static const uint64_t STRINGS_TABLE_HEADER = 0xBDC0;
static const uint64_t FUNCTIONS_TABLE_HEADER = 0xBE80;

FidDatabase::FidDatabase() : loaded(false) {}
FidDatabase::~FidDatabase() {}

bool FidDatabase::parse_header(std::ifstream& file) {
    char magic[16];
    file.seekg(0);
    file.read(magic, 16);
    
    // Verify magic: "/01,4),*" followed by metadata
    if (magic[0] != '/' || magic[1] != '0' || magic[2] != '1') {
        std::cerr << "[FidDatabase] Invalid FIDBF magic" << std::endl;
        return false;
    }
    
    return true;
}

bool FidDatabase::parse_strings_table(std::ifstream& file, uint64_t offset, uint64_t count) {
    // Strings table parsing
    // Format: ID -> null-terminated string
    file.seekg(offset);
    
    // Read table header
    char header[32];
    file.read(header, 32);
    
    // Basic parsing: scan for null-terminated strings
    uint64_t current_id = 0;
    std::string current_str;
    
    while (!file.eof() && current_id < count) {
        char c;
        file.get(c);
        
        if (c == '\0') {
            if (!current_str.empty()) {
                strings_table[current_id++] = current_str;
                current_str.clear();
            }
        } else if (c >= 32 && c < 127) {  // Printable ASCII
            current_str += c;
        }
    }
    
    return true;
}

bool FidDatabase::parse_functions_table(std::ifstream& file, uint64_t offset, uint64_t count) {
    file.seekg(offset);
    
    // Skip header text (variable length, ends with 0xFF sentinel)
    char c;
    while (file.get(c) && c != (char)0xFF) {}
    
    // Read function records
    // Based on reverse engineering:
    // Function ID (4) | Code Unit Size (2) | Full Hash (8) | 
    // Specific Hash Size (1) | Specific Hash (8) | Library ID (4) |
    // Name ID (4) | Entry Point (4) | Flags (1)
    
    for (uint64_t i = 0; i < count && !file.eof(); ++i) {
        FidFunctionRecord rec;
        
        uint32_t func_id;
        file.read(reinterpret_cast<char*>(&func_id), 4);
        rec.function_id = func_id;
        
        file.read(reinterpret_cast<char*>(&rec.code_unit_size), 2);
        file.read(reinterpret_cast<char*>(&rec.full_hash), 8);
        file.read(reinterpret_cast<char*>(&rec.specific_hash_size), 1);
        file.read(reinterpret_cast<char*>(&rec.specific_hash), 8);
        
        uint32_t lib_id, name_id, entry;
        file.read(reinterpret_cast<char*>(&lib_id), 4);
        file.read(reinterpret_cast<char*>(&name_id), 4);
        file.read(reinterpret_cast<char*>(&entry), 4);
        
        rec.library_id = lib_id;
        rec.name_id = name_id;
        rec.entry_point = entry;
        
        file.read(reinterpret_cast<char*>(&rec.flags), 1);
        
        // Resolve name from strings table
        auto it = strings_table.find(rec.name_id);
        if (it != strings_table.end()) {
            rec.name = it->second;
        }
        
        // Add to index
        hash_index.insert({rec.full_hash, functions.size()});
        functions.push_back(rec);
    }
    
    return true;
}

bool FidDatabase::parse_libraries_table(std::ifstream& file, uint64_t offset, uint64_t count) {
    // Libraries table - simplified parsing
    return true;
}

bool FidDatabase::load(const std::string& path) {
    filepath = path;
    loaded = false;
    
    std::ifstream file(path, std::ios::binary);
    if (!file.is_open()) {
        std::cerr << "[FidDatabase] Failed to open: " << path << std::endl;
        return false;
    }
    
    // Get file size
    file.seekg(0, std::ios::end);
    size_t file_size = file.tellg();
    file.seekg(0);
    
    std::cerr << "[FidDatabase] Loading " << path << " (" << file_size << " bytes)" << std::endl;
    
    if (!parse_header(file)) {
        return false;
    }
    
    // Parse strings table first (needed for name resolution)
    parse_strings_table(file, STRINGS_TABLE_HEADER, 100000);
    
    // Parse functions table
    parse_functions_table(file, FUNCTIONS_TABLE_HEADER, 100000);
    
    std::cerr << "[FidDatabase] Loaded " << functions.size() << " functions, " 
              << strings_table.size() << " strings" << std::endl;
    
    loaded = !functions.empty();
    return loaded;
}

std::vector<std::string> FidDatabase::lookup_by_hash(uint64_t full_hash) const {
    std::vector<std::string> results;
    
    auto range = hash_index.equal_range(full_hash);
    for (auto it = range.first; it != range.second; ++it) {
        size_t idx = it->second;
        if (idx < functions.size() && !functions[idx].name.empty()) {
            results.push_back(functions[idx].name);
        }
    }
    
    return results;
}

// FID Hash Calculator
// Based on Ghidra's MessageDigestFidHasher - uses MD5 truncated to 64 bits
uint64_t FidHasher::calculate_full_hash(const uint8_t* bytes, size_t size) {
    // Simplified FNV-1a 64-bit hash as approximation
    // TODO: Implement exact Ghidra hash (MD5 with instruction masking)
    uint64_t hash = 0xcbf29ce484222325ULL;  // FNV offset basis
    const uint64_t prime = 0x100000001b3ULL;
    
    for (size_t i = 0; i < size; ++i) {
        hash ^= bytes[i];
        hash *= prime;
    }
    
    return hash;
}

} // namespace analysis
} // namespace fission
