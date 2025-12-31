/**
 * Fission Decompiler - Encoding Utilities
 * Base64 encoding/decoding
 */

#ifndef FISSION_UTILS_ENCODING_H
#define FISSION_UTILS_ENCODING_H

#include <string>
#include <vector>
#include <cstdint>

namespace fission {
namespace utils {

/**
 * Decode base64 encoded string to bytes
 * @param encoded Base64 encoded string
 * @return Decoded bytes
 */
std::vector<uint8_t> base64_decode(const std::string& encoded);

} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_ENCODING_H
