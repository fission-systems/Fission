#ifndef FISSION_CORE_ARCH_POLICY_H
#define FISSION_CORE_ARCH_POLICY_H

#include "architecture.hh"
#include "type.hh"

namespace fission {
namespace core {

/**
 * Centralized policy for architecture-dependent values.
 * Ensures consistent handling of pointer sizes, alignment, and primitive types
 * across all analyses (Step 4b, 4c, etc.).
 */
class ArchPolicy {
public:
    // Get the default pointer size for the architecture (bytes)
    static int getPointerSize(const ghidra::Architecture* arch) {
        return arch->getDefaultSize();
    }
    
    // Get the default alignment for the architecture (bytes)
    static int getAlignment(const ghidra::Architecture* arch) {
        // Default alignment typically matches pointer size
        return arch->getDefaultSize();
    }
    
    // Get the standard integer type for the architecture (matches pointer size)
    static ghidra::Datatype* getIntType(ghidra::TypeFactory* factory, 
                                         const ghidra::Architecture* arch) {
        return factory->getBase(getPointerSize(arch), ghidra::TYPE_INT);
    }
    
    // Get a pointer type to a base type
    static ghidra::TypePointer* getPointerType(ghidra::TypeFactory* factory,
                                                ghidra::Datatype* base,
                                                const ghidra::Architecture* arch) {
        int ptr_size = getPointerSize(arch);
        return factory->getTypePointer(ptr_size, base, ptr_size);
    }
};

} // namespace core
} // namespace fission

#endif // FISSION_CORE_ARCH_POLICY_H
