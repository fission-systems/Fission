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
    bool apply_memory_blocks = true;
};

void initialize_architecture(fission::ffi::DecompContext* ctx);
void initialize_architecture(fission::ffi::DecompContext* ctx, const ArchInitOptions& options);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_ARCH_INIT_H
