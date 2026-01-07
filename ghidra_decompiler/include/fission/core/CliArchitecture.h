#ifndef FISSION_CORE_CLI_ARCHITECTURE_H
#define FISSION_CORE_CLI_ARCHITECTURE_H

#include <string>
#include <iostream>
#include <map>
#include <vector>
#include <cstdint>
#include "sleigh_arch.hh"
#include "fission/loader/MemoryImage.h"
#include "fission/core/SymbolProvider.h"

namespace fission {
namespace core {

class CliArchitecture : public ghidra::SleighArchitecture {
    ghidra::LoadImage* custom_loader;
    const SymbolProvider* symbol_provider = nullptr;

public:
    CliArchitecture(const std::string& sleigh_id, ghidra::LoadImage* ldr, std::ostream* err);
    ~CliArchitecture() override = default;

    virtual void buildLoader(ghidra::DocumentStorage& store) override;
    virtual ghidra::Scope* buildDatabase(ghidra::DocumentStorage& store) override;

    // Inject IAT symbols into symbol table
    void injectIatSymbols(const std::map<uint64_t, std::string>& symbols);

    void setSymbolProvider(const SymbolProvider* provider);
    void refreshReadOnly();
};

// Helper to configure architecture with advanced options
void configure_arch(CliArchitecture* arch);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_CLI_ARCHITECTURE_H
