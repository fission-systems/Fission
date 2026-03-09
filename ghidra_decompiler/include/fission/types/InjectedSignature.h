/**
 * Shared signature types for type back-propagation injection.
 * Used by both DecompContext (FFI) and PrototypeEnforcer.
 */

#ifndef FISSION_TYPES_INJECTED_SIGNATURE_H
#define FISSION_TYPES_INJECTED_SIGNATURE_H

#include <string>
#include <vector>

namespace fission {
namespace types {

struct InjectedParamInfo {
    std::string name;
    std::string type_name;
};

struct InjectedApiSignature {
    std::string name;
    std::string return_type;
    std::vector<InjectedParamInfo> params;
};

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_INJECTED_SIGNATURE_H
