#include "fission/core/DecompilerContext.h"
#include "libdecomp.hh"
#include "fission/core/ArchPolicy.h"
#include "sleigh_arch.hh"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/utils/file_utils.h"
#include "fission/utils/logger.h"
#include <iostream>

namespace fission {
namespace core {

using namespace fission::types;
using namespace fission::utils;

// Helper function to load GDT types (moved from fission_decomp.cpp)
static void load_gdt_for_arch(ghidra::Architecture* arch, bool is_64bit) {
    std::string suffix = is_64bit ? "_64.gdt" : "_32.gdt";
    std::vector<std::string> candidates = {
        "../../ghidra/typeinfo/win32/windows_vs12" + suffix,
        "../ghidra/typeinfo/win32/windows_vs12" + suffix,
        "./ghidra/typeinfo/win32/windows_vs12" + suffix
    };
    
    for (const auto& path : candidates) {
        if (file_exists(path)) {
            std::cerr << "[DecompilerContext] Loading GDT (" << (is_64bit ? "64-bit" : "32-bit") << ") from: " << path << std::endl;
            GdtBinaryParser gdt;
            if (gdt.load(path)) {
                TypeManager::load_types_from_gdt(arch->types, &gdt, ArchPolicy::getPointerSize(arch));
            }
            break;
        }
    }
}

// Helper function to configure architecture
static void configure_arch(ghidra::Architecture* arch) {
    // Ghidra 11.x API change: setOptionDefault is not available directly on OptionDatabase
    // We should use arch->options->put(...) or similar if needed, but for now let's comment out
    // or use the correct API if we can find it.
    // Assuming these were intended to set default analysis options.
    
    // arch->options->setOptionDefault("allowcontextset", "true");
    // arch->options->setOptionDefault("analyze.aggregates", "true");
    // arch->options->setOptionDefault("decompile.unreachable", "true");
    // arch->options->setOptionDefault("decompile.readonly", "true");
    // arch->options->setOptionDefault("proto.eval", "true");
}

DecompilerContext::DecompilerContext() = default;

DecompilerContext::~DecompilerContext() {
    if (arch_64bit) delete arch_64bit;
    if (arch_32bit) delete arch_32bit;
    if (loader_64bit) delete loader_64bit;
    if (loader_32bit) delete loader_32bit;
}

bool DecompilerContext::initialize(const std::string& sleigh_directory) {
    if (initialized && sla_dir == sleigh_directory) {
        return true;
    }
    
    try {
        ghidra::startDecompilerLibrary(sleigh_directory.c_str());
        
        std::string langDir = sleigh_directory;
        // Check if sleigh_directory already ends with "languages"
        if (langDir.length() < 9 || langDir.substr(langDir.length() - 9) != "languages") {
            langDir += "/languages";
        }
        
        ghidra::SleighArchitecture::specpaths.addDir2Path(langDir);
        ghidra::SleighArchitecture::getDescriptions();
        sla_dir = sleigh_directory;
        initialized = true;
        return true;
    } catch (...) {
        return false;
    }
}

void DecompilerContext::setup_architecture(bool is_64bit, const std::vector<uint8_t>& bytes, uint64_t image_base, const std::string& compiler_id) {
    if (is_64bit) {
        if (!arch_64bit_ready) {
            if (loader_64bit) delete loader_64bit;
            if (arch_64bit) delete arch_64bit;

            loader_64bit = new fission::loader::MemoryLoadImage(bytes, image_base);
            std::string arch_id = "x86:LE:64:default:" + compiler_id;
            arch_64bit = new CliArchitecture(arch_id, loader_64bit, &fission::utils::null_stream());
            ghidra::DocumentStorage store;
            arch_64bit->init(store);
            configure_arch(arch_64bit);
            
            TypeManager::register_windows_types(arch_64bit->types, ArchPolicy::getPointerSize(arch_64bit));
            load_gdt_for_arch(arch_64bit, true);
            
            arch_64bit_ready = true;
            std::cerr << "[DecompilerContext] Initialized 64-bit architecture" << std::endl;
        } else {
            // Only update data if bytes is not empty, otherwise preserve existing binary
            if (!bytes.empty()) {
                loader_64bit->updateData(bytes, image_base);
            }
            arch_64bit->symboltab->getGlobalScope()->clear();
        }
    } else {
        if (!arch_32bit_ready) {
            if (loader_32bit) delete loader_32bit;
            if (arch_32bit) delete arch_32bit;

            loader_32bit = new fission::loader::MemoryLoadImage(bytes, image_base);
            std::string arch_id = "x86:LE:32:default:" + compiler_id;
            arch_32bit = new CliArchitecture(arch_id, loader_32bit, &fission::utils::null_stream());
            ghidra::DocumentStorage store;
            arch_32bit->init(store);
            configure_arch(arch_32bit);
            
            TypeManager::register_windows_types(arch_32bit->types, ArchPolicy::getPointerSize(arch_32bit));
            load_gdt_for_arch(arch_32bit, false);
            
            arch_32bit_ready = true;
            std::cerr << "[DecompilerContext] Initialized 32-bit architecture" << std::endl;
        } else {
            // Only update data if bytes is not empty, otherwise preserve existing binary
            if (!bytes.empty()) {
                loader_32bit->updateData(bytes, image_base);
            }
            arch_32bit->symboltab->getGlobalScope()->clear();
        }
    }
}

} // namespace core
} // namespace fission
