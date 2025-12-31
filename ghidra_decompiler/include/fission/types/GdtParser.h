/**
 * Fission Decompiler - GDT Parser
 * Parses Ghidra Data Type (GDT) information from JSON
 */

#ifndef FISSION_TYPES_GDT_PARSER_H
#define FISSION_TYPES_GDT_PARSER_H

#include <string>
#include <vector>
#include <map>
#include <cstdint>

namespace fission {
namespace types {

// Structure to hold GDT type information
struct GdtField {
    std::string name;
    int offset;
    int size;
};

struct GdtStruct {
    std::string name;
    int size;
    int alignment;
    std::vector<GdtField> fields;
};

struct GdtTypedef {
    std::string alias;
    std::string base;
};

struct GdtEnum {
    std::string name;
    uint64_t value;
};

struct GdtData {
    std::vector<GdtStruct> structs;
    std::vector<GdtTypedef> typedefs;
    std::vector<GdtEnum> enums;
};

/**
 * Parse GDT JSON data
 * @param json JSON string containing GDT data
 * @return Parsed GDT data structure
 */
GdtData parse_gdt_json(const std::string& json);

/**
 * Load enum values from GDT data
 * @param gdt_data Parsed GDT data
 * @return Map of value -> enum name
 */
std::map<uint64_t, std::string> load_gdt_enums(const GdtData& gdt_data);

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_GDT_PARSER_H
