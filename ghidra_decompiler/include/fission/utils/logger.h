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
 * Simple Logger for centralized output control
 */
class Logger {
public:
    static void initialize(const std::string& filename = "");
    
    // Direct log (just message)
    static void log(const std::string& msg);
    
    // Leveled logging
    static void info(const std::string& msg);
    static void warn(const std::string& msg);
    static void error(const std::string& msg);
    
    // Stream-based logging (returns ostream that logs on destruction)
    static std::ostream& stream();
};

/**
 * Get null output stream (discards all output)
 */
std::ostream& null_stream();

/**
 * Get logging stream (replacement for std::cerr)
 */
std::ostream& log_stream();

} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_LOGGER_H
