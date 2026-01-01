#ifndef __INTERNAL_SIGNATURES_H__
#define __INTERNAL_SIGNATURES_H__

#include <map>
#include <string>
#include <vector>
#include <cstdint>

namespace fission {
namespace analysis {

/// \brief Signature for internal function recognition
struct InternalSignature {
    std::string name;                   ///< Suggested function name
    std::vector<std::string> strings;   ///< Required string references
    std::vector<uint8_t> prologue;      ///< Expected prologue bytes
    int min_size;                       ///< Minimum function size
    int max_size;                       ///< Maximum function size
    
    InternalSignature() : min_size(0), max_size(0x10000) {}
};

/// \brief Pattern matcher for internal functions
///
/// Uses string references and prologue patterns to identify
/// common internal functions (PyInstaller, upx, etc.)
class InternalMatcher {
private:
    std::vector<InternalSignature> signatures;
    std::map<uint64_t, std::string> matched;

    void load_pyinstaller_signatures();
    void load_common_crt_signatures();

public:
    InternalMatcher();
    ~InternalMatcher();

    /// Match a function based on its string references
    /// \param address Function address
    /// \param strings List of strings referenced by this function
    /// \return Matched name or empty string
    std::string match_by_strings(uint64_t address, const std::vector<std::string>& strings);

    /// Match a function based on its prologue bytes
    /// \param address Function address
    /// \param bytes Prologue bytes
    /// \param size Number of bytes
    /// \return Matched name or empty string
    std::string match_by_prologue(uint64_t address, const uint8_t* bytes, int size);

    /// Get all matched functions
    const std::map<uint64_t, std::string>& get_matches() const { return matched; }

    /// Get signature count
    size_t get_signature_count() const { return signatures.size(); }
};

} // namespace analysis
} // namespace fission

#endif // __INTERNAL_SIGNATURES_H__
