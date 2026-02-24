#ifndef __VTABLE_ANALYZER_H__
#define __VTABLE_ANALYZER_H__

#include <cstdint>
#include <map>
#include <vector>
#include <string>

// Forward declarations for Ghidra types (used in register_vtable_types)
namespace ghidra {
    class Architecture;
}

namespace fission {
namespace analysis {

/**
 * Virtual function entry
 */
struct VirtualFunction {
    int slot_index;             // Position in vtable (0, 1, 2, ...)
    uint64_t function_addr;     // Target function address
    std::string name;           // Resolved name (e.g., "Foo::bar")
    bool is_pure_virtual;       // Is this a pure virtual (points to __purecall)?
};

/**
 * Virtual function table
 */
struct VTable {
    uint64_t address;           // VTable address in .rdata
    std::string class_name;     // Class name (from RTTI if available)
    std::vector<VirtualFunction> entries;
    uint64_t rtti_pointer;      // RTTI locator address (if present)
    bool has_rtti;
};

/**
 * VTable reference in constructor
 */
struct VTableRef {
    uint64_t constructor_addr;  // Constructor function
    uint64_t vtable_addr;       // VTable being assigned
    int offset_in_object;       // Offset where vtable ptr is stored (usually 0)
};

/**
 * VTableAnalyzer - C++ Virtual Function Table Recovery
 * 
 * Detects vtables in .rdata section and links them to classes via RTTI.
 * Also identifies indirect calls through vtables and provides resolution.
 */
class VTableAnalyzer {
public:
    VTableAnalyzer();
    ~VTableAnalyzer();

    /**
     * Scan memory image for vtable patterns
     * @param data Binary data
     * @param size Binary size  
     * @param image_base Image base address
     * @param is_64bit 64-bit binary?
     */
    void scan_vtables(const uint8_t* data, size_t size, uint64_t image_base, bool is_64bit);
    
    /**
     * Link vtables with RTTI class names
     * @param rtti_classes Map of address -> class name from RttiAnalyzer
     */
    void link_with_rtti(const std::map<uint64_t, std::string>& rtti_classes);
    
    /**
     * Try to resolve an indirect call through vtable
     * @param object_addr Base address of object (usually from register)
     * @param vtable_offset Offset from object to vtable pointer
     * @param slot_offset Offset within vtable (e.g., 0x10 = slot 2 for 64-bit)
     * @return Resolved function address, or 0 if unknown
     */
    uint64_t resolve_virtual_call(uint64_t vtable_addr, int slot_offset, int ptr_size) const;
    
    /**
     * Get all detected vtables
     */
    const std::vector<VTable>& get_vtables() const { return vtables; }
    
    /**
     * Get vtable by address
     */
    const VTable* get_vtable(uint64_t addr) const;
    
    /**
     * Get function name for indirect call annotation
     */
    std::string get_virtual_call_name(uint64_t vtable_addr, int slot_offset, int ptr_size) const;
    
    /**
     * Clear all data
     */
    void clear();

    /**
     * P2-A: Register vtable layouts into Ghidra's TypeFactory and global scope.
     *
     * For each detected vtable this creates:
     *   - A TypeStruct named "vtbl_<class_name>" with one TypeCode* field per slot.
     *   - A TypePointer to that struct.
     *   - A global symbol at the vtable address with the struct pointer type.
     *
     * This allows ActionInferTypes to propagate vcall object types automatically.
     *
     * @param arch  Ghidra Architecture (provides types + symboltab)
     */
    void register_vtable_types(ghidra::Architecture* arch);

private:
    std::vector<VTable> vtables;
    std::map<uint64_t, size_t> vtable_index; // addr -> index in vtables
    
    // Helper: check if address looks like a function
    bool looks_like_function_ptr(uint64_t addr, uint64_t image_base, size_t binary_size) const;
    
    // Helper: scan for consecutive function pointers
    bool scan_vtable_at(const uint8_t* data, size_t offset, size_t max_size,
                        uint64_t image_base, size_t binary_size, int ptr_size, VTable& out);
};

} // namespace analysis
} // namespace fission

#endif
