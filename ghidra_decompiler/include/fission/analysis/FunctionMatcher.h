#ifndef __FUNCTION_MATCHER_H__
#define __FUNCTION_MATCHER_H__

#include <map>
#include <vector>
#include <string>
#include <cstdint>

namespace fission {
namespace analysis {

/// \brief A single function signature entry
struct FunctionSignature {
    std::string name;               ///< Function name (e.g., "malloc")
    std::string library;            ///< Library name (e.g., "msvcrt")
    std::vector<uint8_t> pattern;   ///< Byte pattern (first N bytes of function)
    std::vector<uint8_t> mask;      ///< Mask for wildcard bytes (0xFF = exact, 0x00 = wildcard)
    int pattern_length;             ///< Length of pattern
    
    FunctionSignature() : pattern_length(0) {}
};

/// \brief Pattern-based function signature matcher
///
/// This is a lightweight alternative to Ghidra's FID system.
/// It matches function prologues against a database of known signatures.
class FunctionMatcher {
private:
    std::vector<FunctionSignature> signatures;      ///< Loaded signatures
    std::map<uint64_t, std::string> matched_funcs;  ///< Address -> function name

    /// Load built-in MSVC x64 signatures
    void load_builtin_msvc_x64();

    /// Match a single pattern against bytes
    bool match_pattern(const uint8_t* bytes, int size, const FunctionSignature& sig) const;

public:
    FunctionMatcher();
    ~FunctionMatcher();

    /// Load signatures from JSON file
    bool load_signatures(const std::string& json_path);

    /// Load built-in signatures for a specific platform
    void load_builtin_signatures(const std::string& platform);

    /// Match function bytes and return name if matched
    std::string match(uint64_t address, const uint8_t* bytes, int size);

    /// Get all matched functions
    const std::map<uint64_t, std::string>& get_matches() const { return matched_funcs; }

    /// Get signature count
    size_t get_signature_count() const { return signatures.size(); }
};

} // namespace analysis
} // namespace fission

#endif // __FUNCTION_MATCHER_H__
