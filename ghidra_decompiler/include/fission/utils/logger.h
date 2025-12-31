/**
 * Fission Decompiler - Logging Utilities
 */

#ifndef FISSION_UTILS_LOGGER_H
#define FISSION_UTILS_LOGGER_H

#include <iostream>
#include <streambuf>

namespace fission {
namespace utils {

/**
 * Null buffer to silence log output
 */
class NullBuffer : public std::streambuf {
public:
    int overflow(int c) { return c; }
};

/**
 * Get null output stream (discards all output)
 */
std::ostream& null_stream();

} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_LOGGER_H
