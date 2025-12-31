/**
 * Fission Decompiler - GDT Parser Implementation
 */

#include "fission/types/GdtParser.h"
#include "fission/utils/json_utils.h"
#include <iostream>

namespace fission {
namespace types {

using namespace fission::utils;

GdtData parse_gdt_json(const std::string& json) {
    GdtData data;
    
    // Parse typedefs: "typedefs":[{"alias":"...","base":"..."}]
    std::string typedefs_key = "\"typedefs\":[";
    size_t typedefs_pos = json.find(typedefs_key);
    if (typedefs_pos != std::string::npos) {
        typedefs_pos += typedefs_key.length();
        while (typedefs_pos < json.length()) {
            if (json[typedefs_pos] == ']') break;
            
            size_t end_obj = json.find('}', typedefs_pos);
            if (end_obj == std::string::npos) break;
            
            std::string obj_str = json.substr(typedefs_pos, end_obj - typedefs_pos + 1);
            std::string alias = extract_json_string(obj_str, "alias");
            std::string base = extract_json_string(obj_str, "base");
            
            if (!alias.empty() && !base.empty()) {
                data.typedefs.push_back({alias, base});
            }
            
            typedefs_pos = end_obj + 1;
            while (typedefs_pos < json.length() && (json[typedefs_pos] == ',' || json[typedefs_pos] == ' ' || json[typedefs_pos] == '\n')) typedefs_pos++;
        }
    }
    
    // Parse structs: "structs":[{"name":"...","size":...,"alignment":...,"fields":[...]}]
    std::string structs_key = "\"structs\":[";
    size_t structs_pos = json.find(structs_key);
    if (structs_pos != std::string::npos) {
        structs_pos += structs_key.length();
        while (structs_pos < json.length()) {
            if (json[structs_pos] == ']') break;
            
            // Note: Structs contain nested arrays (fields), so simple '}' search is risky
            // But GDT JSON is strictly formatted, so we can try a basic counter
            size_t struct_end = structs_pos;
            int depth = 0;
            while (struct_end < json.length()) {
                if (json[struct_end] == '{') depth++;
                else if (json[struct_end] == '}') {
                    depth--;
                    if (depth == 0) break;
                }
                struct_end++;
            }
            if (struct_end >= json.length()) break;
            
            std::string struct_json = json.substr(structs_pos, struct_end - structs_pos + 1);
            GdtStruct s;
            s.name = extract_json_string(struct_json, "name");
            s.size = (int)extract_json_int(struct_json, "size");
            s.alignment = (int)extract_json_int(struct_json, "alignment");
            
            // Parse fields
            std::string fields_key = "\"fields\":[";
            size_t fields_pos = struct_json.find(fields_key);
            if (fields_pos != std::string::npos) {
                fields_pos += fields_key.length();
                while (fields_pos < struct_json.length()) {
                    if (struct_json[fields_pos] == ']') break;
                    
                    size_t field_end = struct_json.find('}', fields_pos);
                    if (field_end == std::string::npos) break;
                    
                    std::string field_str = struct_json.substr(fields_pos, field_end - fields_pos + 1);
                    GdtField f;
                    f.name = extract_json_string(field_str, "name");
                    f.offset = (int)extract_json_int(field_str, "offset");
                    f.size = (int)extract_json_int(field_str, "size");
                    s.fields.push_back(f);
                    
                    fields_pos = field_end + 1;
                     while (fields_pos < struct_json.length() && (struct_json[fields_pos] == ',' || struct_json[fields_pos] == ' ')) fields_pos++;
                }
            }
            
            if (!s.name.empty()) {
                data.structs.push_back(s);
            }
            
            structs_pos = struct_end + 1;
            while (structs_pos < json.length() && (json[structs_pos] == ',' || json[structs_pos] == ' ' || json[structs_pos] == '\n')) structs_pos++;
        }
    }

    // Parse enums: "enums":[{"name":"...","value":...}]
    std::string enums_key = "\"enums\":[";
    size_t enums_pos = json.find(enums_key);
    if (enums_pos != std::string::npos) {
        enums_pos += enums_key.length();
        while (enums_pos < json.length()) {
            if (json[enums_pos] == ']') break;
            
            size_t end_obj = json.find('}', enums_pos);
            if (end_obj == std::string::npos) break;
            
            std::string obj_str = json.substr(enums_pos, end_obj - enums_pos + 1);
            std::string name = extract_json_string(obj_str, "name");
            int64_t value = extract_json_int(obj_str, "value");
            
            if (!name.empty()) {
                data.enums.push_back({name, (uint64_t)value});
            }
            
            enums_pos = end_obj + 1;
            while (enums_pos < json.length() && (json[enums_pos] == ',' || json[enums_pos] == ' ' || json[enums_pos] == '\n')) enums_pos++;
        }
    }
    
    return data;
}

std::map<uint64_t, std::string> load_gdt_enums(const GdtData& gdt_data) {
    std::map<uint64_t, std::string> enum_values;
    for (const auto& e : gdt_data.enums) {
        enum_values[e.value] = e.name;
    }
    return enum_values;
}

} // namespace types
} // namespace fission
