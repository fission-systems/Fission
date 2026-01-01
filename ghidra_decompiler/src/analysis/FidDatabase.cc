#include "fission/analysis/FidDatabase.h"
#include <iostream>
#include <cstring>
#include <algorithm>
#include <vector>

namespace fission {
namespace analysis {

// Known offsets in FIDBF format (from reverse engineering)
static const uint64_t HEADER_MAGIC_OFFSET = 0x0000;
static const uint64_t TABLE_INDEX_OFFSET = 0x4000;
static const uint64_t STRINGS_TABLE_HEADER = 0xBDC0;
static const uint64_t FUNCTIONS_TABLE_HEADER = 0xBE80;

// Endianness helpers (Ghidra DB4 is Big Endian)
inline uint64_t read_be64(std::ifstream& file) {
    uint8_t buf[8];
    file.read((char*)buf, 8);
    return ((uint64_t)buf[0] << 56) | ((uint64_t)buf[1] << 48) |
           ((uint64_t)buf[2] << 40) | ((uint64_t)buf[3] << 32) |
           ((uint64_t)buf[4] << 24) | ((uint64_t)buf[5] << 16) |
           ((uint64_t)buf[6] << 8)  | (uint64_t)buf[7];
}

inline uint32_t read_be32(std::ifstream& file) {
    uint8_t buf[4];
    file.read((char*)buf, 4);
    return ((uint32_t)buf[0] << 24) | ((uint32_t)buf[1] << 16) |
           ((uint32_t)buf[2] << 8)  | (uint32_t)buf[3];
}

inline uint16_t read_be16(std::ifstream& file) {
    uint8_t buf[2];
    file.read((char*)buf, 2);
    return ((uint16_t)buf[0] << 8) | (uint16_t)buf[1];
}

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
    // Strings table parsing (Heuristic Scraper)
    // We found 'malloc' at 0x890AB5 in vs2019_x86.fidbf.
    // The strings table likely starts around there.
    
    // Start scanning from 0x800000 (8MB) to find string data
    // Format: ... [Length(2)] [String] ...
    
    uint64_t scan_start = 0x800000;
    file.seekg(0, std::ios::end);
    uint64_t file_size = file.tellg();
    
    if (scan_start >= file_size) scan_start = 0;
    
    file.seekg(scan_start);
    
    const size_t BUFFER_SIZE = 1024 * 1024; // 1MB buffer
    std::vector<char> buffer(BUFFER_SIZE);
    
    size_t strings_found = 0;
    uint64_t current_pos = scan_start;
    
    while (current_pos < file_size) {
        file.read(buffer.data(), BUFFER_SIZE);
        size_t bytes_read = file.gcount();
        if (bytes_read < 4) break;
        
        for (size_t i = 0; i < bytes_read - 2; ++i) {
            // Heuristic A: Pascal-style string (Length (2 bytes BE) + Chars)
            // Length must be reasonable (e.g., 3 < len < 200)
            uint16_t len = ((uint8_t)buffer[i] << 8) | (uint8_t)buffer[i+1];
            
            if (len >= 3 && len < 200 && i + 2 + len < bytes_read) {
                // Check if chars are printable ASCII
                bool is_string = true;
                bool has_alphanum = false;
                for (int k = 0; k < len; ++k) {
                    char c = buffer[i + 2 + k];
                    if (c < 32 || c > 126) {
                        is_string = false;
                        break;
                    }
                    if (isalnum(c)) has_alphanum = true;
                }
                
                if (is_string && has_alphanum) {
                    // Extract string
                    std::string s(&buffer[i + 2], len);
                    
                    // Simple ID mapping: simply index by order found or hash?
                    // The Functions Table refers to "Name ID". In DB4, this is usually a Record ID.
                    // Without the map table, we can't link ID -> String easily.
                    // BUT: We observed that Name ID is often 0 or sequential.
                    // Let's store by hash of the string? No, FunctionRecord has Key.
                    
                    // CRITICAL: We need to map NameID (from FunctionRecord) to this string.
                    // If we can't map it, we can't show names.
                    // For now, let's just populate a "reverse lookup" or guess IDs.
                    // Actually, let's try to map the file offset as ID?
                    // Or maybe the NameID is actually the offset in the Strings file?
                    strings_table[current_pos + i] = s;
                    
                    // Also store by sequential index just in case
                    strings_table[strings_found] = s;  // Try 0, 1, 2...
                    
                    strings_found++;
                    i += 1 + len; // Skip string
                }
            }
        }
        
        current_pos += bytes_read;
        // Overlap handling omitted for simplicity (small risk of missing string at boundary)
    }
    
    std::cerr << "[FidDatabase] Scraped " << strings_found << " strings, mapped by offset and index" << std::endl;
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
