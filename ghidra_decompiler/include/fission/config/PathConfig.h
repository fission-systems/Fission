/**
 * Fission Path Configuration
 * 
 * Centralized configuration for file paths and signature locations.
 */

#ifndef FISSION_CONFIG_PATH_CONFIG_H
#define FISSION_CONFIG_PATH_CONFIG_H

#include <string>
#include <vector>

namespace fission {
namespace config {

    // Common file extensions
    static const std::string EXT_FID = ".fidbf";
    static const std::string EXT_GDT = ".gdt";

    // FID Database retrieval
    std::string get_fid_filename(bool is_64bit, const std::string& compiler_id);
    std::vector<std::string> get_all_fid_paths(bool is_64bit);
    std::string find_fid_file(const std::string& filename);

    // GDT Database retrieval
    std::vector<std::string> get_gdt_candidates(bool is_64bit);
    std::string find_gdt_file(const std::string& filename);

    // Common Symbols
    std::vector<std::string> get_common_symbol_files();

    // GUID/IID Files
    std::vector<std::string> get_guid_files();

} // namespace config
} // namespace fission

#endif // FISSION_CONFIG_PATH_CONFIG_H
