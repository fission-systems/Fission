#ifndef __DATA_SECTION_SCANNER_H__
#define __DATA_SECTION_SCANNER_H__

#include <vector>
#include <string>
#include <cstdint>

namespace ghidra {
    class Datatype;
}

namespace fission {
namespace loaders {

/// \brief Information about a detected data symbol
struct DataSymbol {
    uint64_t address;        ///< Virtual address of the data
    int size;                ///< Size in bytes (4 for float, 8 for double, variable for strings)
    int type_meta;           ///< TYPE_FLOAT, TYPE_ARRAY, etc.
    std::string type_id;     ///< Type identifier ("float8", "float4", "char")
    std::string name;        ///< Symbol name (e.g., "DAT_1400040c8")
    uint64_t raw_value;      ///< Raw bytes as uint64 (not used for strings)
};

/// \brief Scanner for data section to detect strings, floats, and other constants
///
/// Scans binary data sections (.rdata, .data) to automatically detect
/// strings, floating-point constants, and other data that should have symbols.
/// This enables proper type propagation through memory loads and string inlining.
class DataSectionScanner {
public:
    DataSectionScanner();
    ~DataSectionScanner();
    
    /// \brief Scan a data section for recognizable patterns
    /// \param data Raw binary data
    /// \param section_va Virtual address where section is loaded
    /// \param section_size Size of the section in bytes
    /// \return Vector of detected data symbols
    std::vector<DataSymbol> scanDataSection(
        const uint8_t* data,
        uint64_t section_va,
        size_t section_size
    );
    
    /// \brief Check if a 64-bit value looks like a valid double
    /// \param bits Raw 64-bit value
    /// \return true if it looks like a reasonable double value
    bool looksLikeDouble(uint64_t bits);
    
    /// \brief Check if a 32-bit value looks like a valid float
    /// \param bits Raw 32-bit value
    /// \return true if it looks like a reasonable float value
    bool looksLikeFloat(uint32_t bits);
    
    /// \brief Check if data at offset looks like a null-terminated ASCII string
    /// \param data Raw binary data
    /// \param offset Starting offset
    /// \param section_size Total section size
    /// \param string_length Output parameter for detected string length
    /// \return true if it looks like a valid string
    bool looksLikeAsciiString(const uint8_t* data, size_t offset, size_t section_size, size_t& string_length);
    
private:
    /// \brief Generate symbol name for an address
    /// \param address Virtual address
    /// \return Symbol name like "DAT_1400040c8"
    std::string generateSymbolName(uint64_t address);
    
    /// \brief Check if a floating-point value is in reasonable range
    /// \param value The float/double value
    /// \return true if in reasonable range (not too extreme)
    bool isReasonableValue(double value);
};

} // namespace loaders
} // namespace fission

#endif // __DATA_SECTION_SCANNER_H__
