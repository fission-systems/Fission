#ifndef FISSION_CORE_ARCH_INIT_H
#define FISSION_CORE_ARCH_INIT_H

namespace fission {
namespace ffi {
struct DecompContext;
}

namespace core {

struct ArchInitOptions {
    bool apply_feature_flags = true;
    bool register_windows_types = true;
    bool load_gdt = true;
    bool inject_symbols = true;
    bool register_functions = true;
    bool read_loader_symbols = true;
    bool apply_memory_blocks = true;
    bool register_data_symbols = true;  // FISSION IMPROVEMENT: Scan and register data section symbols
};

void initialize_architecture(fission::ffi::DecompContext* ctx);
void initialize_architecture(fission::ffi::DecompContext* ctx, const ArchInitOptions& options);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_ARCH_INIT_H
