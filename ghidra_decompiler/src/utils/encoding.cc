/**
 * Fission Decompiler - Encoding Utilities Implementation
 */

#include "fission/utils/encoding.h"

namespace fission {
namespace utils {

static const std::string base64_chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

std::vector<uint8_t> base64_decode(const std::string& encoded) {
    std::vector<uint8_t> result;
    int val = 0, bits = -8;
    for (unsigned char c : encoded) {
        if (c == '=') break;
        size_t pos = base64_chars.find(c);
        if (pos == std::string::npos) {
            continue; // Skip invalid chars
        }
        val = (val << 6) + pos;
        bits += 6;
        if (bits >= 0) {
            result.push_back((val >> bits) & 0xFF);
            bits -= 8;
        }
    }
    return result;
}

} // namespace utils
} // namespace fission
