#ifndef __BINARY_READER_H__
#define __BINARY_READER_H__

#include <cstdint>
#include <fstream>
#include <string>
#include <cstring>
#include <vector>

namespace fission {
namespace util {

/// \brief Big-Endian binary reader utilities
/// Common functions for parsing Ghidra DB4 format files (GDT, FIDBF)
class BinaryReader {
public:
    /// Read 64-bit unsigned integer (Big Endian)
    static inline uint64_t read_be64(std::ifstream& file) {
        uint8_t buf[8];
        file.read(reinterpret_cast<char*>(buf), 8);
        return ((uint64_t)buf[0] << 56) | ((uint64_t)buf[1] << 48) |
               ((uint64_t)buf[2] << 40) | ((uint64_t)buf[3] << 32) |
               ((uint64_t)buf[4] << 24) | ((uint64_t)buf[5] << 16) |
               ((uint64_t)buf[6] << 8)  | (uint64_t)buf[7];
    }

    /// Read 32-bit unsigned integer (Big Endian)
    static inline uint32_t read_be32(std::ifstream& file) {
        uint8_t buf[4];
        file.read(reinterpret_cast<char*>(buf), 4);
        return ((uint32_t)buf[0] << 24) | ((uint32_t)buf[1] << 16) |
               ((uint32_t)buf[2] << 8)  | (uint32_t)buf[3];
    }

    /// Read 16-bit unsigned integer (Big Endian)
    static inline uint16_t read_be16(std::ifstream& file) {
        uint8_t buf[2];
        file.read(reinterpret_cast<char*>(buf), 2);
        return ((uint16_t)buf[0] << 8) | (uint16_t)buf[1];
    }

    /// Read Java-style UTF string (2-byte BE length + chars)
    static inline std::string read_java_utf(std::ifstream& file) {
        uint16_t length = read_be16(file);
        std::string result(length, '\0');
        file.read(&result[0], length);
        return result;
    }

    /// Read 64-bit signed integer (Big Endian) - Java long
    static inline int64_t read_java_long(std::ifstream& file) {
        return static_cast<int64_t>(read_be64(file));
    }

    /// Read 32-bit signed integer (Big Endian) - Java int
    static inline int32_t read_java_int(std::ifstream& file) {
        return static_cast<int32_t>(read_be32(file));
    }

    /// Find table by name pattern in memory buffer
    /// \return offset of pattern, or SIZE_MAX if not found
    static size_t find_table_header(const std::vector<char>& data, 
                                    const char* table_name, 
                                    size_t name_len) {
        for (size_t i = 0; i < data.size() - name_len; ++i) {
            if (memcmp(&data[i], table_name, name_len) == 0) {
                return i;
            }
        }
        return SIZE_MAX;
    }

    /// Find 0xFFFFFFFF sentinel after offset
    /// \return offset after sentinel, or SIZE_MAX if not found
    static size_t find_sentinel(const std::vector<char>& data, size_t start, size_t max_search = 200) {
        size_t end = std::min(start + max_search, data.size() - 4);
        for (size_t j = start; j < end; ++j) {
            if ((uint8_t)data[j] == 0xFF && (uint8_t)data[j+1] == 0xFF && 
                (uint8_t)data[j+2] == 0xFF && (uint8_t)data[j+3] == 0xFF) {
                return j + 4; // Skip sentinel
            }
        }
        return SIZE_MAX;
    }

    /// Check if string contains only valid identifier characters
    static bool is_valid_identifier(const std::string& s) {
        if (s.empty() || s.length() > 256) return false;
        if (!std::isalpha(s[0]) && s[0] != '_') return false;
        for (char c : s) {
            if (!std::isalnum(c) && c != '_') return false;
        }
        return true;
    }
};

} // namespace util
} // namespace fission

#endif // __BINARY_READER_H__
