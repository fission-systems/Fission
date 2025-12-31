#ifndef FISSION_PROCESSING_POST_PROCESSORS_H
#define FISSION_PROCESSING_POST_PROCESSORS_H

#include <string>
#include <map>
#include <vector>
#include <cstdint>

namespace fission {
namespace processing {

// Function to post-process IAT calls
std::string post_process_iat_calls(const std::string& code, const std::map<uint64_t, std::string>& iat_symbols);

// Function to inline strings
std::string inline_strings(const std::string& code, const std::map<uint64_t, std::string>& string_table);

// Apply function signatures
std::string apply_function_signatures(const std::string& code);


// Smart constant replacement
std::string smart_constant_replace(const std::string& code);

// Fallback constant replacement
std::string post_process_constants(const std::string& code, const std::map<uint64_t, std::string>& enum_values);

// Substitute GUID strings with names
std::string substitute_guids(const std::string& code, const std::map<std::string, std::string>& guid_map);

// Recover unicode strings from raw byte arrays
std::string recover_unicode_strings(const std::string& code);

} // namespace processing
} // namespace fission

#endif // FISSION_PROCESSING_POST_PROCESSORS_H
