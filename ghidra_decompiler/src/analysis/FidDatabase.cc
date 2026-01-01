#include "fission/analysis/FidDatabase.h"
#include "fission/util/BinaryReader.h"
#include <iostream>
#include <cstring>
#include <algorithm>
#include <vector>

namespace fission {
namespace analysis {

using fission::util::BinaryReader;

// Convenience aliases
inline uint64_t read_be64(std::ifstream& file) { return BinaryReader::read_be64(file); }
inline uint32_t read_be32(std::ifstream& file) { return BinaryReader::read_be32(file); }
inline uint16_t read_be16(std::ifstream& file) { return BinaryReader::read_be16(file); }

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
    // DB4 format uses paged storage - string data is NOT contiguous after header
    // We'll try direct parsing first, then fall back to heuristic scraping
    
    file.seekg(offset);
    
    size_t parsed = 0;
    size_t max_parse = std::min(count, (uint64_t)200000);
    
    // Try direct Key(8)+Len(2)+Value parsing
    while (parsed < max_parse && !file.eof()) {
        uint64_t key = read_be64(file);
        if (file.eof()) break;
        
        uint16_t len = read_be16(file);
        if (file.eof()) break;
        
        // Sanity check
        if (len > 1024 || len == 0) {
            break; // Data format doesn't match, try heuristic
        }
        
        std::string value;
        value.resize(len);
        file.read(&value[0], len);
        
        if (file.gcount() != len) break;
        
        strings_table[key] = value;
        parsed++;
        
        if (parsed <= 3) {
            std::cerr << "[FidDatabase] String #" << parsed << ": key=0x" << std::hex << key 
                      << " \"" << value << "\"" << std::dec << std::endl;
        }
    }
    
    // If direct parsing failed, use heuristic scraping from file
    if (parsed < 100) {
        std::cerr << "[FidDatabase] Direct parsing got " << parsed << " strings, trying heuristic scan..." << std::endl;
        
        // Scan from ~50% of file size to find string data blocks
        file.clear();
        file.seekg(0, std::ios::end);
        size_t file_size = file.tellg();
        
        // Start scanning from 50% of file (strings usually in second half for FIDBF)
        size_t scan_start = file_size / 2;
        file.seekg(scan_start);
        
        const size_t BUFFER_SIZE = 1024 * 1024; // 1MB chunks
        std::vector<char> buffer(BUFFER_SIZE);
        size_t strings_found = 0;
        
        while (file.tellg() < (std::streampos)(file_size - 10) && strings_found < max_parse) {
            size_t pos = file.tellg();
            file.read(buffer.data(), BUFFER_SIZE);
            size_t bytes_read = file.gcount();
            if (bytes_read < 20) break;
            
            for (size_t i = 0; i < bytes_read - 12; ++i) {
                // Look for pattern: Key(8) + Len(2, BE) + printable ASCII
                uint16_t len = ((uint8_t)buffer[i+8] << 8) | (uint8_t)buffer[i+9];
                
                if (len >= 3 && len < 200 && i + 10 + len <= bytes_read) {
                    bool is_valid = true;
                    bool has_alpha = false;
                    for (size_t k = 0; k < len; ++k) {
                        char c = buffer[i + 10 + k];
                        if (c < 32 || c > 126) { is_valid = false; break; }
                        if (std::isalpha(c)) has_alpha = true;
                    }
                    
                    if (is_valid && has_alpha) {
                        uint64_t key = ((uint64_t)(uint8_t)buffer[i] << 56) | 
                                       ((uint64_t)(uint8_t)buffer[i+1] << 48) |
                                       ((uint64_t)(uint8_t)buffer[i+2] << 40) | 
                                       ((uint64_t)(uint8_t)buffer[i+3] << 32) |
                                       ((uint64_t)(uint8_t)buffer[i+4] << 24) | 
                                       ((uint64_t)(uint8_t)buffer[i+5] << 16) |
                                       ((uint64_t)(uint8_t)buffer[i+6] << 8)  | 
                                       (uint64_t)(uint8_t)buffer[i+7];
                        
                        std::string s(&buffer[i + 10], len);
                        strings_table[key] = s;
                        strings_found++;
                        
                        if (strings_found <= 5) {
                            std::cerr << "[FidDatabase] Heuristic String #" << strings_found 
                                      << ": key=0x" << std::hex << key << " \"" << s << "\"" << std::dec << std::endl;
                        }
                        
                        i += 9 + len; // Skip past this string
                    }
                }
            }
        }
        
        parsed = strings_found;
    }
    
    std::cerr << "[FidDatabase] Parsed " << strings_table.size() << " strings" << std::endl;
    return true;
}



bool FidDatabase::parse_functions_table(std::ifstream& file, uint64_t offset, uint64_t count) {
    file.seekg(offset);
    
    // Skip header text (variable length, ends with 0xFF sentinel)
    char c;
    while (file.get(c) && c != (char)0xFF) {}
    
    // Read function records
    // Verified Schema (FunctionsTable.java):
    // Key: Function ID (8 bytes)
    // Data:
    // 0: Code Unit Size (2)
    // 1: Full Hash (8)
    // 2: Specific Hash Additional Size (1)
    // 3: Specific Hash (8)
    // 4: Library ID (8)
    // 5: Name ID (8)
    // 6: Entry Point (8)
    // 7: Domain Path ID (8)
    // 8: Flags (1)
    
    size_t debug_count = 0;
    
    for (uint64_t i = 0; i < count && !file.eof(); ++i) {
        FidFunctionRecord rec;
        
        // Key (8 bytes)
        rec.function_id = read_be64(file);
        
        // Data
        rec.code_unit_size = read_be16(file);
        rec.full_hash = read_be64(file);
        
        file.read(reinterpret_cast<char*>(&rec.specific_hash_size), 1);
        rec.specific_hash = read_be64(file);
        
        rec.library_id = read_be64(file);
        rec.name_id = read_be64(file);
        rec.entry_point = read_be64(file);
        rec.domain_path_id = read_be64(file);
        
        file.read(reinterpret_cast<char*>(&rec.flags), 1);
        
        if (file.eof()) break;

        // Resolve name from strings table
        auto it = strings_table.find(rec.name_id);
        if (it != strings_table.end()) {
            rec.name = it->second;
        }
        
        // Debug: Print first 5 records with names
        if (!rec.name.empty() && debug_count < 5) {
            std::cerr << "[FidDatabase] Sample record #" << i << ": " 
                      << rec.name << " hash=0x" << std::hex << rec.full_hash 
                      << " size=" << std::dec << rec.code_unit_size 
                      << " lib_id=" << rec.library_id << std::endl;
            debug_count++;
        }
        
        // Add to index
        // Use full_hash as key for lookup
        if (rec.full_hash != 0) {  // Only index valid hashes
            hash_index.insert({rec.full_hash, functions.size()});
            functions.push_back(rec);
        }
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
    
    // Read file into memory for pattern searching
    std::vector<char> data(file_size);
    file.seekg(0);
    file.read(data.data(), file_size);
    
    // Find "Strings Table" header - schema: "String ID;String Value"
    const char* strings_marker = "Strings Table";
    size_t strings_table_offset = 0;
    uint32_t strings_count = 0;
    
    for (size_t i = 0; i < file_size - 50; ++i) {
        if (memcmp(&data[i], strings_marker, 13) == 0) {
            // Found "Strings Table", find 0xFFFFFFFF sentinel after schema
            for (size_t j = i; j < std::min(i + 200, file_size - 12); ++j) {
                if ((uint8_t)data[j] == 0xFF && (uint8_t)data[j+1] == 0xFF && 
                    (uint8_t)data[j+2] == 0xFF && (uint8_t)data[j+3] == 0xFF) {
                    
                    // After sentinel: [8 bytes hash?] [4 bytes count BE]
                    // Read count from j+4+8 = j+12
                    if (j + 16 < file_size) {
                        strings_count = ((uint8_t)data[j+12] << 24) | ((uint8_t)data[j+13] << 16) |
                                        ((uint8_t)data[j+14] << 8) | (uint8_t)data[j+15];
                        if (strings_count > 500000) strings_count = 100000; // sanity
                    }
                    
                    // Data starts after the header info block
                    // Skip: sentinel(4) + hash(8) + count(4) + unknown(4) = 20 bytes
                    strings_table_offset = j + 20;
                    break;
                }
            }
            
            if (strings_table_offset > 0) {
                std::cerr << "[FidDatabase] Found Strings Table at 0x" << std::hex 
                          << strings_table_offset << " (count=" << std::dec << strings_count << ")" << std::endl;
                break;
            }
        }
    }
    
    // Find "Functions Table" header
    const char* funcs_marker = "Functions Table";
    size_t functions_table_offset = 0;
    uint32_t functions_count = 0;
    
    for (size_t i = 0; i < file_size - 50; ++i) {
        if (memcmp(&data[i], funcs_marker, 15) == 0) {
            // Found "Functions Table"
            for (size_t j = i; j < std::min(i + 200, file_size - 12); ++j) {
                if ((uint8_t)data[j] == 0xFF && (uint8_t)data[j+1] == 0xFF && 
                    (uint8_t)data[j+2] == 0xFF && (uint8_t)data[j+3] == 0xFF) {
                    
                    // Read count from j+12
                    if (j + 16 < file_size) {
                        functions_count = ((uint8_t)data[j+12] << 24) | ((uint8_t)data[j+13] << 16) |
                                          ((uint8_t)data[j+14] << 8) | (uint8_t)data[j+15];
                        if (functions_count > 500000) functions_count = 100000;
                    }
                    
                    functions_table_offset = j + 20;
                    break;
                }
            }
            
            if (functions_table_offset > 0) {
                std::cerr << "[FidDatabase] Found Functions Table at 0x" << std::hex 
                          << functions_table_offset << " (count=" << std::dec << functions_count << ")" << std::endl;
                break;
            }
        }
    }
    
    // Parse strings table first (needed for name resolution)
    if (strings_table_offset > 0) {
        file.clear();
        file.seekg(0);
        parse_strings_table(file, strings_table_offset, strings_count > 0 ? strings_count : 100000);
    }
    
    // Parse functions table
    if (functions_table_offset > 0) {
        file.clear();
        file.seekg(0);
        parse_functions_table(file, functions_table_offset, functions_count > 0 ? functions_count : 100000);
    }
    
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

// Lookup function by name pattern (for debugging/alternative matching)
std::string FidDatabase::lookup_name_contains(const std::string& pattern) const {
    for (const auto& func : functions) {
        if (func.name.find(pattern) != std::string::npos) {
            return func.name;
        }
    }
    return "";
}

// Get some sample hashes for debugging
void FidDatabase::print_sample_hashes(size_t count) const {
    std::cerr << "[FidDatabase] Sample hashes from database:" << std::endl;
    for (size_t i = 0; i < std::min(count, functions.size()); ++i) {
        const auto& f = functions[i];
        if (!f.name.empty()) {
            std::cerr << "  " << f.name << " -> 0x" << std::hex << f.full_hash << std::dec << std::endl;
        }
    }
}

// FID Hash Calculator
// Based on Ghidra's FNV1a64MessageDigest (from generic/hash/FNV1a64MessageDigest.java)
// FNV-1a 64-bit hash algorithm

// FNV-1a constants (from Ghidra source)
static const uint64_t FNV_64_OFFSET_BASIS = 0xcbf29ce484222325ULL;
static const uint64_t FNV_64_PRIME = 1099511628211ULL;  // 0x100000001b3

uint64_t FidHasher::calculate_full_hash(const uint8_t* bytes, size_t size) {
    // Mask instruction operands for position-independent hashing
    std::vector<uint8_t> masked(bytes, bytes + size);
    
    // Simple x86 operand masking (mask immediate/displacement operands)
    // Ghidra uses InstructionPrototype.getInstructionMask() - we approximate
    for (size_t i = 0; i < size; ) {
        uint8_t op = masked[i];
        
        // CALL rel32 (E8 xx xx xx xx) - mask offset
        if (op == 0xE8 && i + 4 < size) {
            masked[i+1] = masked[i+2] = masked[i+3] = masked[i+4] = 0x00;
            i += 5;
            continue;
        }
        // JMP rel32 (E9 xx xx xx xx) - mask offset
        if (op == 0xE9 && i + 4 < size) {
            masked[i+1] = masked[i+2] = masked[i+3] = masked[i+4] = 0x00;
            i += 5;
            continue;
        }
        // Jcc rel32 (0F 8x xx xx xx xx) - mask offset
        if (op == 0x0F && i + 5 < size && (masked[i+1] & 0xF0) == 0x80) {
            masked[i+2] = masked[i+3] = masked[i+4] = masked[i+5] = 0x00;
            i += 6;
            continue;
        }
        // MOV reg, imm32 (B8-BF xx xx xx xx) - mask immediate
        if ((op >= 0xB8 && op <= 0xBF) && i + 4 < size) {
            masked[i+1] = masked[i+2] = masked[i+3] = masked[i+4] = 0x00;
            i += 5;
            continue;
        }
        // PUSH imm32 (68 xx xx xx xx) - mask immediate
        if (op == 0x68 && i + 4 < size) {
            masked[i+1] = masked[i+2] = masked[i+3] = masked[i+4] = 0x00;
            i += 5;
            continue;
        }
        
        i++;
    }
    
    // Compute FNV-1a 64-bit hash (matches Ghidra's FNV1a64MessageDigest.update/digestLong)
    uint64_t hashvalue = FNV_64_OFFSET_BASIS;
    for (size_t i = 0; i < masked.size(); ++i) {
        hashvalue ^= (masked[i] & 0xff);
        hashvalue *= FNV_64_PRIME;
    }
    
    return hashvalue;
}

} // namespace analysis
} // namespace fission
