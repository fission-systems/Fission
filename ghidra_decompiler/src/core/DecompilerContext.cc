#include "fission/core/DecompilerContext.h"
#include "libdecomp.hh"
#include "sleigh_arch.hh"

namespace fission {
namespace core {

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
        std::string langDir = sleigh_directory + "/languages";
        ghidra::SleighArchitecture::specpaths.addDir2Path(langDir);
        ghidra::SleighArchitecture::getDescriptions();
        sla_dir = sleigh_directory;
        initialized = true;
        return true;
    } catch (...) {
        return false;
    }
}

} // namespace core
} // namespace fission
