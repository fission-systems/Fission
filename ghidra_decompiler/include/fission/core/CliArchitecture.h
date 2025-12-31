#ifndef FISSION_CORE_CLI_ARCHITECTURE_H
#define FISSION_CORE_CLI_ARCHITECTURE_H

#include <string>
#include <iostream>
#include <map>
#include <vector>
#include <cstdint>
#include "sleigh_arch.hh"
#include "fission/loader/MemoryImage.h"

namespace fission {
namespace core {

class CliArchitecture : public ghidra::SleighArchitecture {
    fission::loader::MemoryLoadImage* custom_loader;

public:
    CliArchitecture(const std::string& sleigh_id, fission::loader::MemoryLoadImage* ldr, std::ostream* err);
    virtual ~CliArchitecture() = default;

    virtual void buildLoader(ghidra::DocumentStorage& store) override;

    // Inject IAT symbols into symbol table
    void injectIatSymbols(const std::map<uint64_t, std::string>& symbols);
};

// Helper to configure architecture with advanced options
void configure_arch(CliArchitecture* arch);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_CLI_ARCHITECTURE_H
