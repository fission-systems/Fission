/**
 * Fission Decompiler - JSON Utilities
 * Minimal JSON parsing (no external dependencies)
 */

#ifndef FISSION_UTILS_JSON_UTILS_H
#define FISSION_UTILS_JSON_UTILS_H

#include <string>
#include <vector>
#include <map>
#include <cstdint>

namespace fission {
namespace utils {

/**
 * Extract string value from JSON
 * @param json JSON string
 * @param key Key to search for
 * @return Value or empty string if not found
 */
std::string extract_json_string(const std::string& json, const std::string& key);

/**
 * Extract integer value from JSON
 * @param json JSON string
 * @param key Key to search for
 * @return Value or 0 if not found
 */
int64_t extract_json_int(const std::string& json, const std::string& key);

/**
 * Extract boolean value from JSON
 * @param json JSON string
 * @param key Key to search for
 * @return Value or false if not found
 */
bool extract_json_bool(const std::string& json, const std::string& key);

/**
 * Escape string for JSON output
 * @param s String to escape
 * @return Escaped string
 */
std::string json_escape(const std::string& s);

/**
 * Extract IAT symbols from JSON object
 * Format: {"iat_symbols":{"0x401000":"GetProcAddress",...}}
 * @param json JSON string containing iat_symbols
 * @return Map of address -> function name
 */
std::map<uint64_t, std::string> extract_iat_symbols(const std::string& json);

/**
 * Generic JSON Map Parser
 * Parses a subset of JSON: a flat object containing string keys (parsed as addresses) and string values.
 * e.g. {"0x401000": "funcName", "4096": "label"}
 * 
 * @param json_content The content to parse (must start with {)
 * @param start_pos Position to start parsing from (default 0)
 * @return Map of address -> name
 */
std::map<uint64_t, std::string> parse_json_string_map(const std::string& json_content, size_t start_pos = 0);

/**
 * Extract JSON array elements
 * Parses a JSON array and returns each object as a string.
 * e.g. [{"name":"a"},{"name":"b"}] -> ["{"name":"a"}", "{"name":"b"}"]
 * 
 * @param json_content The JSON array content (must start with [)
 * @return Vector of JSON object strings
 */
std::vector<std::string> extract_json_array(const std::string& json_content);


} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_JSON_UTILS_H
