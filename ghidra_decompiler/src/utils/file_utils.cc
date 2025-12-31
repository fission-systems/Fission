#include "fission/utils/file_utils.h"
#include <fstream>
#include <sstream>

namespace fission {
namespace utils {

std::string read_file_content(const std::string& path) {
    if (path.empty()) return "";
    std::ifstream f(path);
    if (!f.is_open()) return "";
    std::stringstream buffer;
    buffer << f.rdbuf();
    return buffer.str();
}

bool file_exists(const std::string& path) {
    std::ifstream f(path);
    return f.good();
}

} // namespace utils
} // namespace fission
