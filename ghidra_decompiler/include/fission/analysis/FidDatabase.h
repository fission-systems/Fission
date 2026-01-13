#ifndef __FID_DATABASE_H__
#define __FID_DATABASE_H__

#include <map>
#include <set>
#include <unordered_set>
#include <string>
#include <vector>
#include <cstdint>
#include <fstream>

namespace fission {
namespace analysis {

/// \brief Single function record from FID database
struct FidFunctionRecord {
    uint64_t function_id;       ///< Record ID
    uint16_t code_unit_size;    ///< Number of code units
    uint64_t full_hash;         ///< Full hash (8 bytes)
    uint8_t specific_hash_size; ///< Additional size for specific hash
    uint64_t specific_hash;     ///< Specific hash (8 bytes)
    uint64_t library_id;        ///< Library ID
    uint64_t name_id;           ///< String table index for name
    uint64_t entry_point;       ///< Entry point offset
    uint64_t domain_path_id;    ///< Domain path string ID
    uint8_t flags;              ///< Flags (auto-pass, auto-fail, etc.)
    
    std::string name;           ///< Resolved function name
};

/// \brief Library record from FID database
struct FidLibraryRecord {
    uint64_t library_id;
    std::string family_name;
    std::string version;
    std::string language_id;    ///< e.g., "x86:LE:64:default"
};

/// \brief Ghidra FID Database (.fidbf) Parser
///
/// Parses the packed Ghidra Function ID database format
/// to enable function signature matching without Ghidra Java runtime.
class FidDatabase {
private:
    std::string filepath;
    bool loaded;
    
    // In-memory tables
    std::map<uint64_t, std::string> strings_table;
    std::vector<FidLibraryRecord> libraries;
    std::vector<FidFunctionRecord> functions;
    
    // Hash lookup index (full_hash -> function records)
    std::multimap<uint64_t, size_t> hash_index;
    
    // Common symbols filter (loaded from common_symbols_*.txt)
    std::set<std::string> common_symbols;
    
    // Parse the packed DB4 format
    bool parse_header(std::ifstream& file);
    bool parse_strings_table(std::ifstream& file, uint64_t offset, uint64_t count);
    bool parse_functions_table(std::ifstream& file, uint64_t offset, uint64_t count);
    bool parse_libraries_table(std::ifstream& file, uint64_t offset, uint64_t count);
    
    // Load common symbols filter
    bool load_common_symbols(const std::string& filter_path);

    // Relation table storage (Superior Table keys)
    // Key format: (CallerID * PRIME) ^ CalleeFullHash
    std::unordered_set<uint64_t> superior_relations;
    
    // Parse relations table
    bool parse_relations_table(std::ifstream& file, uint64_t offset, uint64_t count);

public:
    FidDatabase();
    ~FidDatabase();

    /// Load a .fidbf file
    bool load(const std::string& filepath);

    /// Check if database is loaded
    bool is_loaded() const { return loaded; }

    /// Get total function count
    size_t get_function_count() const { return functions.size(); }

    /// Lookup function by full hash (filtered by common symbols)
    /// \return vector of matching function names
    std::vector<std::string> lookup_by_hash(uint64_t full_hash) const;

    /// Lookup function records by full hash 
    /// \return vector of matching function records (pointers to internal storage)
    std::vector<const FidFunctionRecord*> lookup_records_by_hash(uint64_t full_hash) const;
    
    /// Lookup function by full hash + specific hash (more accurate)
    /// \return vector of matching function names
    std::vector<std::string> lookup_by_hashes(uint64_t full_hash, uint64_t specific_hash) const;
    
    /// Check if a symbol is in the common symbols filter
    bool is_common_symbol(const std::string& name) const;
    
    /// Check if a relation exists (CallerID -> CalleeHash)
    /// \param caller_id The Function ID of the caller (from FidFunctionRecord)
    /// \param callee_hash The Full Hash of the callee
    bool has_relation(uint64_t caller_id, uint64_t callee_hash) const;
    
    /// Lookup function by name pattern (contains check)
    std::string lookup_name_contains(const std::string& pattern) const;
    
    /// Print sample hashes for debugging
    void print_sample_hashes(size_t count = 10) const;

    /// Get all functions (for debugging)
    const std::vector<FidFunctionRecord>& get_all_functions() const { return functions; }
};

/// \brief FID Hash Calculator
///
/// Calculates the same hash as Ghidra's MessageDigestFidHasher
class FidHasher {
public:
    /// Calculate full hash from masked instruction bytes
    static uint64_t calculate_full_hash(const uint8_t* bytes, size_t size);
    
    /// Calculate specific hash (first 5 bytes for additional matching)
    /// Ghidra uses this to reduce false positives
    static uint64_t calculate_specific_hash(const uint8_t* bytes, size_t size);
};

} // namespace analysis
} // namespace fission

#endif // __FID_DATABASE_H__
