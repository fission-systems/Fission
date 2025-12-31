#ifndef FISSION_PROCESSING_CONSTANTS_H
#define FISSION_PROCESSING_CONSTANTS_H

#include <string>
#include <vector>
#include <map>
#include <cstdint>

namespace fission {
namespace processing {

// Enum group definitions for flag resolution
extern std::map<std::string, std::map<uint64_t, std::string>> ENUM_GROUPS;

struct ApiParamMapping {
    std::string func_name;
    int param_index;
    std::string enum_group;
};

extern std::vector<ApiParamMapping> API_PARAM_MAPPINGS;

struct ApiSignature {
    std::vector<std::string> param_names;
};

extern std::map<std::string, ApiSignature> API_SIGNATURES;

// Dynamic flag combination resolver
std::string resolve_flag_combination(uint64_t value, const std::map<uint64_t, std::string>& group);

} // namespace processing
} // namespace fission

#endif // FISSION_PROCESSING_CONSTANTS_H
