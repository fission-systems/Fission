#include "fission/core/ScopeFission.h"

#include "architecture.hh"
#include "varnode.hh"

namespace fission {
namespace core {

ScopeFission::ScopeFission(ghidra::Architecture* arch, const SymbolProvider* provider)
    : ghidra::ScopeInternal(0, "", arch),
      symbol_provider_(provider) {}

void ScopeFission::set_symbol_provider(const SymbolProvider* provider) {
    symbol_provider_ = provider;
    hole_ranges_.clear();
}

ghidra::SymbolEntry* ScopeFission::findAddr(
    const ghidra::Address& addr,
    const ghidra::Address& usepoint
) const {
    if (ghidra::SymbolEntry* entry = ghidra::ScopeInternal::findAddr(addr, usepoint)) {
        return entry;
    }

    if (ghidra::SymbolEntry* entry = ghidra::ScopeInternal::findContainer(addr, 1, ghidra::Address())) {
        (void)entry;
        return nullptr;
    }

    if (is_hole(addr, 1)) {
        return nullptr;
    }

    if (addr.getSpace() != glb->getDefaultDataSpace()) {
        return nullptr;
    }

    if (!symbol_provider_) {
        return nullptr;
    }

    SymbolInfo info;
    if (!symbol_provider_->find_symbol(addr.getOffset(), 1, true, info)) {
        add_hole(addr, 1);
        return nullptr;
    }

    auto* mutable_self = const_cast<ScopeFission*>(this);
    mutable_self->cache_data_symbol(info, usepoint);
    return ghidra::ScopeInternal::findAddr(addr, usepoint);
}

ghidra::SymbolEntry* ScopeFission::findContainer(
    const ghidra::Address& addr,
    ghidra::int4 size,
    const ghidra::Address& usepoint
) const {
    if (ghidra::SymbolEntry* entry = ghidra::ScopeInternal::findContainer(addr, size, usepoint)) {
        return entry;
    }

    ghidra::int4 normalized_size = size > 0 ? size : 1;
    if (is_hole(addr, normalized_size)) {
        return nullptr;
    }

    if (addr.getSpace() != glb->getDefaultDataSpace()) {
        return nullptr;
    }

    if (!symbol_provider_) {
        return nullptr;
    }

    SymbolInfo info;
    if (!symbol_provider_->find_symbol(addr.getOffset(), static_cast<uint32_t>(normalized_size), false, info)) {
        add_hole(addr, normalized_size);
        return nullptr;
    }

    auto* mutable_self = const_cast<ScopeFission*>(this);
    mutable_self->cache_data_symbol(info, usepoint);
    return ghidra::ScopeInternal::findContainer(addr, size, usepoint);
}

ghidra::Funcdata* ScopeFission::findFunction(const ghidra::Address& addr) const {
    if (ghidra::Funcdata* fd = ghidra::ScopeInternal::findFunction(addr)) {
        return fd;
    }

    if (ghidra::SymbolEntry* entry = ghidra::ScopeInternal::findContainer(addr, 1, ghidra::Address())) {
        (void)entry;
        return nullptr;
    }

    if (is_hole(addr, 1)) {
        return nullptr;
    }

    if (addr.getSpace() != glb->getDefaultCodeSpace()) {
        return nullptr;
    }

    if (!symbol_provider_) {
        return nullptr;
    }

    SymbolInfo info;
    if (!symbol_provider_->find_function(addr.getOffset(), info)) {
        add_hole(addr, 1);
        return nullptr;
    }

    auto* mutable_self = const_cast<ScopeFission*>(this);
    mutable_self->cache_function_symbol(info);
    return ghidra::ScopeInternal::findFunction(addr);
}

void ScopeFission::cache_data_symbol(
    const SymbolInfo& info,
    const ghidra::Address& usepoint
) {
    ghidra::AddrSpace* data_space = glb->getDefaultDataSpace();
    if (!data_space) {
        return;
    }

    ghidra::Address sym_addr(data_space, info.address);
    ghidra::int4 size = info.size > 0 ? static_cast<ghidra::int4>(info.size) : 1;
    ghidra::Datatype* ct = glb->types->getBase(size, ghidra::TYPE_UNKNOWN);

    ghidra::SymbolEntry* entry = addSymbol(info.name, ct, sym_addr, usepoint);
    if (!entry) {
        return;
    }

    ghidra::Symbol* sym = entry->getSymbol();
    if (!sym) {
        return;
    }

    if ((info.flags & SymbolFlagReadOnly) != 0) {
        setAttribute(sym, ghidra::Varnode::readonly);
    }
    if ((info.flags & SymbolFlagVolatile) != 0) {
        setAttribute(sym, ghidra::Varnode::volatil);
    }
}

void ScopeFission::cache_function_symbol(const SymbolInfo& info) {
    ghidra::AddrSpace* code_space = glb->getDefaultCodeSpace();
    if (!code_space) {
        return;
    }

    ghidra::Address sym_addr(code_space, info.address);
    addFunction(sym_addr, info.name);
}

bool ScopeFission::is_hole(const ghidra::Address& addr, ghidra::int4 size) const {
    if (size <= 0) {
        return false;
    }
    return hole_ranges_.inRange(addr, size);
}

void ScopeFission::add_hole(const ghidra::Address& addr, ghidra::int4 size) const {
    if (size <= 0 || addr.isInvalid()) {
        return;
    }
    ghidra::uintb first = addr.getOffset();
    ghidra::uintb last = first + static_cast<ghidra::uintb>(size - 1);
    if (last < first) {
        last = first;
    }
    hole_ranges_.insertRange(addr.getSpace(), first, last);
}

} // namespace core
} // namespace fission
