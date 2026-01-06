#ifndef FISSION_CORE_SCOPE_FISSION_H
#define FISSION_CORE_SCOPE_FISSION_H

#include "database.hh"
#include "types.h"
#include "address.hh"
#include "fission/core/SymbolProvider.h"

namespace fission {
namespace core {

class ScopeFission final : public ghidra::ScopeInternal {
public:
    ScopeFission(ghidra::Architecture* arch, const SymbolProvider* provider);

    void set_symbol_provider(const SymbolProvider* provider);

    ghidra::SymbolEntry* findAddr(
        const ghidra::Address& addr,
        const ghidra::Address& usepoint
    ) const override;

    ghidra::SymbolEntry* findContainer(
        const ghidra::Address& addr,
        ghidra::int4 size,
        const ghidra::Address& usepoint
    ) const override;

    ghidra::Funcdata* findFunction(const ghidra::Address& addr) const override;

private:
    const SymbolProvider* symbol_provider_;
    mutable ghidra::RangeList hole_ranges_;

    void cache_data_symbol(
        const SymbolInfo& info,
        const ghidra::Address& usepoint
    );

    void cache_function_symbol(const SymbolInfo& info);

    bool is_hole(const ghidra::Address& addr, ghidra::int4 size) const;
    void add_hole(const ghidra::Address& addr, ghidra::int4 size) const;
};

} // namespace core
} // namespace fission

#endif // FISSION_CORE_SCOPE_FISSION_H
