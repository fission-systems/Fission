#ifndef __GDT_BINARY_PARSER_H__
#define __GDT_BINARY_PARSER_H__

#include <map>
#include <string>
#include <vector>
#include <cstdint>
#include <fstream>

namespace fission {
namespace types {

/// \brief Parsed function prototype from GDT
struct GdtFunctionProto {
    std::string name;               ///< Function name
    std::string return_type;        ///< Return type name
    std::vector<std::string> param_types;  ///< Parameter types
    std::vector<std::string> param_names;  ///< Parameter names
    std::string calling_convention; ///< __stdcall, __cdecl, etc.
};

/// \brief Parsed data type from GDT
struct GdtDataType {
    std::string name;
    std::string category;       ///< struct, enum, typedef, pointer, etc.
    int size;
    int alignment;
    std::string base_type;      ///< For typedef/pointer
    std::vector<std::pair<std::string, int>> fields;  ///< name, offset
};

/// \brief Ghidra GDT Binary File Parser
///
/// Parses the native Ghidra packed database (.gdt) format
/// which uses Java ObjectOutputStream + ZIP compression.
class GdtBinaryParser {
private:
    std::string filepath;
    bool loaded;
    
    // Ghidra packed file constants
    static constexpr uint64_t MAGIC_NUMBER = 0x2e30212634e92c20ULL;
    static constexpr int FORMAT_VERSION = 1;
    
    // Parsed data
    std::string item_name;
    std::string content_type;
    int file_type;
    uint64_t content_length;
    
    std::map<std::string, GdtFunctionProto> functions;
    std::map<std::string, GdtDataType> types;

    // Parse helpers
    bool parse_header(std::ifstream& file);
    bool decompress_content(std::ifstream& file, std::vector<uint8_t>& out);
    bool parse_db_content(const std::vector<uint8_t>& data);
    void parse_function_definitions(const std::vector<uint8_t>& data);
    void parse_parameters(const std::vector<uint8_t>& data);
    
    // Java ObjectOutputStream format helpers
    std::string read_java_utf(std::ifstream& file);
    int32_t read_java_int(std::ifstream& file);
    int64_t read_java_long(std::ifstream& file);

public:
    GdtBinaryParser();
    ~GdtBinaryParser();

    /// Load a .gdt file directly
    bool load(const std::string& filepath);

    /// Check if loaded
    bool is_loaded() const { return loaded; }

    /// Get parsed function prototypes
    const std::map<std::string, GdtFunctionProto>& get_functions() const { return functions; }
    
    /// Get parsed data types
    const std::map<std::string, GdtDataType>& get_types() const { return types; }
    
    /// Lookup function by name
    const GdtFunctionProto* find_function(const std::string& name) const;
    
    /// Lookup type by name
    const GdtDataType* find_type(const std::string& name) const;
    
    /// Get item name
    const std::string& get_item_name() const { return item_name; }
    
    /// Get content type
    const std::string& get_content_type() const { return content_type; }
};

} // namespace types
} // namespace fission

#endif // __GDT_BINARY_PARSER_H__
