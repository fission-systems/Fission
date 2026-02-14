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
std::string normalize_mingw_printf_args(const std::string& code);


// Smart constant replacement
std::string smart_constant_replace(const std::string& code);

// Fallback constant replacement
std::string post_process_constants(const std::string& code, const std::map<uint64_t, std::string>& enum_values);

// Substitute GUID strings with names
std::string substitute_guids(const std::string& code, const std::map<std::string, std::string>& guid_map);

// Recover unicode strings from raw byte arrays
std::string recover_unicode_strings(const std::string& code);

// Replace LOCK/UNLOCK + increment/decrement patterns with Interlocked APIs
std::string replace_interlocked_patterns(const std::string& code);

// Replace xunknown/undefined types with standard Windows types  
std::string standardize_variable_names(const std::string& code);
std::string replace_xunknown_types(const std::string& code);

// Clean up SEH boilerplate (FS_OFFSET, iRam/pcRam naming)
std::string cleanup_seh_boilerplate(const std::string& code);

// Apply global data symbol names (g_/gp_ -> actual symbol names)
std::string apply_global_symbols(
    const std::string& code,
    const std::map<uint64_t, std::string>& global_symbols
);

// Improve internal function names (func_0x → sub_)
std::string improve_internal_function_names(const std::string& code);

// Apply FID-resolved function names (sub_XXXX → actual name)
std::string apply_fid_names(const std::string& code, const std::map<uint64_t, std::string>& fid_names);

// Add inline comments for structure field accesses
std::string annotate_structure_offsets(const std::string& code);

// Demangle C++ names and standardize 'this' pointer
std::string demangle_cpp_names(const std::string& code);

// Normalize noisy C++ indirect/vtable call patterns for readability
std::string normalize_cpp_virtual_calls(const std::string& code);

} // namespace processing
} // namespace fission

#endif // FISSION_PROCESSING_POST_PROCESSORS_H
