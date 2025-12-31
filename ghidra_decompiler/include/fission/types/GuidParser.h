#ifndef FISSION_TYPES_GUID_PARSER_H
#define FISSION_TYPES_GUID_PARSER_H

#include <string>
#include <map>
#include <vector>

namespace fission {
namespace types {

// Loads GUIDs/IIDs from a text file into a map.
// Format: {UUID} {Name}
// Example: 00000000-0000-0000-C000-000000000046 IUnknown
std::map<std::string, std::string> load_guids_to_map(const std::string& content);

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_GUID_PARSER_H
