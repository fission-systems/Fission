#include "fission/core/DecompilerContext.h"
#include "libdecomp.hh"
#include "fission/core/ArchPolicy.h"
#include "sleigh_arch.hh"
#include "architecture.hh"
#include "options.hh"
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
        "../../utils/signatures/typeinfo/win32/windows_vs12" + suffix,
        "../utils/signatures/typeinfo/win32/windows_vs12" + suffix,
        "./utils/signatures/typeinfo/win32/windows_vs12" + suffix
    };
    
    for (const auto& path : candidates) {
        if (file_exists(path)) {
            fission::utils::log_stream() << "[DecompilerContext] Loading GDT (" << (is_64bit ? "64-bit" : "32-bit") << ") from: " << path << std::endl;
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
    if (arch == nullptr || arch->options == nullptr) {
        return;
    }

    try {
        // Match practical defaults already used in FFI path.
        arch->options->set(ghidra::ELEM_INFERCONSTPTR.getId(), "on", "", "");
        arch->options->set(ghidra::ELEM_ANALYZEFORLOOPS.getId(), "on", "", "");
        arch->options->set(ghidra::ELEM_READONLY.getId(), "on", "", "");
        arch->options->set(ghidra::ELEM_JUMPLOAD.getId(), "on", "", "");
        arch->options->set(ghidra::ELEM_ERRORTOOMANYINSTRUCTIONS.getId(), "off", "", "");
        arch->options->set(ghidra::ELEM_INLINE.getId(), "off", "", "");
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerContext] configure_arch option sync failed: "
                  << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[DecompilerContext] configure_arch option sync failed (unknown)" << std::endl;
    }
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

void DecompilerContext::setup_architecture(bool is_64bit, const std::vector<uint8_t>& bytes, uint64_t image_base, const std::string& compiler_id, const std::string& sleigh_id) {
    if (is_64bit) {
        if (!arch_64bit_ready) {
            if (loader_64bit) delete loader_64bit;
            if (arch_64bit) delete arch_64bit;

            loader_64bit = new fission::loader::MemoryLoadImage(bytes, image_base);
            // Use caller-supplied sleigh_id when available (e.g. "AARCH64:LE:64:v8A"),
            // otherwise fall back to x86:LE:64 default.
            std::string arch_id = (sleigh_id.empty() ? "x86:LE:64:default" : sleigh_id)
                                   + ":" + compiler_id;
            arch_64bit = new CliArchitecture(arch_id, loader_64bit, &fission::utils::null_stream());
            ghidra::DocumentStorage store;
            arch_64bit->init(store);
            configure_arch(arch_64bit);

            // A-1: Only inject Windows-specific types (HANDLE, HWND, DWORD…) for
            // PE/MSVC binaries. Loading them for ELF or Mach-O taints the type DB
            // and degrades analysis quality on non-Windows targets.
            if (compiler_id == "windows" || compiler_id == "msvc") {
                TypeManager::register_windows_types(arch_64bit->types, ArchPolicy::getPointerSize(arch_64bit));
            }
            load_gdt_for_arch(arch_64bit, true);

            arch_64bit_ready = true;
            fission::utils::log_stream() << "[DecompilerContext] Initialized 64-bit architecture" << std::endl;
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
            // Use caller-supplied sleigh_id when available (e.g. "ARM:LE:32:v7"),
            // otherwise fall back to x86:LE:32 default.
            std::string arch_id = (sleigh_id.empty() ? "x86:LE:32:default" : sleigh_id)
                                   + ":" + compiler_id;
            arch_32bit = new CliArchitecture(arch_id, loader_32bit, &fission::utils::null_stream());
            ghidra::DocumentStorage store;
            arch_32bit->init(store);
            configure_arch(arch_32bit);

            // A-1: Same guard as 64-bit path — skip Windows types for non-PE targets.
            if (compiler_id == "windows" || compiler_id == "msvc") {
                TypeManager::register_windows_types(arch_32bit->types, ArchPolicy::getPointerSize(arch_32bit));
            }
            load_gdt_for_arch(arch_32bit, false);

            arch_32bit_ready = true;
            fission::utils::log_stream() << "[DecompilerContext] Initialized 32-bit architecture" << std::endl;
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
