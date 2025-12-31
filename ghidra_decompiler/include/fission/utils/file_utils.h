#ifndef FISSION_UTILS_FILE_UTILS_H
#define FISSION_UTILS_FILE_UTILS_H

#include <string>

namespace fission {
namespace utils {

// Read entire file content into a string
std::string read_file_content(const std::string& path);

// Check if file exists
bool file_exists(const std::string& path);

} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_FILE_UTILS_H
