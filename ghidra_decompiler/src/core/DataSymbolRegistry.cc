#include "fission/core/DataSymbolRegistry.h"
#include "fission/loaders/DataSectionScanner.h"
#include "fission/ffi/DecompContext.h"
#include "architecture.hh"
#include "database.hh"
#include "type.hh"
#include <cstring>
#include <iostream>
#include <unordered_set>
#include "fission/utils/logger.h"

namespace fission {
namespace core {

using namespace ghidra;
using namespace fission::loaders;

/// \brief Register data section symbols in global scope
///
/// Scans data sections (.rdata, .data, .rodata, __const) for floating-point constants
/// and registers them as symbols in the global scope with proper types.
/// This enables type propagation through memory loads.
///
/// \param ctx Decompiler context with loaded binary
int registerDataSymbolsInGlobalScope(
    ghidra::Architecture* arch,
    const std::vector<DataSymbol>& symbols,
    const std::function<void(const DataSymbol&)>& on_scanned_symbol
) {
    if (arch == nullptr) {
        return 0;
    }

    Scope* globalScope = arch->symboltab->getGlobalScope();
    TypeFactory* types = arch->types;
    AddrSpace* ramSpace = arch->getDefaultDataSpace();

    if (globalScope == nullptr || types == nullptr || ramSpace == nullptr) {
        fission::utils::log_stream() << "[DataSymbolRegistry] Missing required components" << std::endl;
        return 0;
    }

    int registered_count = 0;
    for (const auto& sym : symbols) {
        if (on_scanned_symbol) {
            on_scanned_symbol(sym);
        }

        try {
            // Get or create appropriate type
            Datatype* dt = nullptr;
            if (sym.type_meta == 9) {  // TYPE_FLOAT
                if (sym.size == 8) {
                    dt = types->getBase(8, TYPE_FLOAT);  // double
                } else if (sym.size == 4) {
                    dt = types->getBase(4, TYPE_FLOAT);  // float
                }
            } else if (sym.type_meta == 11) {  // TYPE_ARRAY (for strings)
                Datatype* charType = types->getBase(1, TYPE_INT);  // char is 1-byte integer
                if (charType != nullptr) {
                    dt = types->getTypeArray(sym.size, charType);
                    fission::utils::log_stream() << "[DataSymbolRegistry] Creating char[" << sym.size
                              << "] type for string at 0x" << std::hex << sym.address << std::dec << std::endl;
                }
            }

            if (dt == nullptr) {
                fission::utils::log_stream() << "[DataSymbolRegistry] Could not create type for symbol at 0x"
                          << std::hex << sym.address << std::dec << std::endl;
                continue;
            }

            // Create address
            Address addr(ramSpace, sym.address);

            // Check if symbol already exists
            SymbolEntry* existing = globalScope->queryContainer(addr, 1, Address());
            if (existing != nullptr) {
                continue;
            }

            // Add new symbol
            SymbolEntry* entry = globalScope->addSymbol(sym.name, dt, addr, Address());
            if (entry != nullptr) {
                registered_count++;
                fission::utils::log_stream() << "[DataSymbolRegistry] Registered symbol: " << sym.name
                          << " at 0x" << std::hex << sym.address
                          << " type=" << dt->getName() << std::dec << std::endl;
            }
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[DataSymbolRegistry] Exception while registering symbol at 0x"
                      << std::hex << sym.address << ": " << e.what() << std::dec << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[DataSymbolRegistry] Unknown exception while registering symbol at 0x"
                      << std::hex << sym.address << std::dec << std::endl;
        }
    }

    return registered_count;
}

// ---------------------------------------------------------------------------
// PE section parsing helper
// ---------------------------------------------------------------------------

std::vector<PeDataSection> extract_pe_data_sections(
    const uint8_t* data,
    size_t         size,
    uint64_t       image_base
) {
    std::vector<PeDataSection> result;

    // Minimum viable PE: DOS stub + PE header + at least one section entry
    if (!data || size < 0x200) return result;

    // DOS magic
    if (data[0] != 'M' || data[1] != 'Z') return result;

    uint32_t pe_offset = 0;
    std::memcpy(&pe_offset, data + 0x3C, sizeof(uint32_t));

    if (pe_offset + 0x180u > size) return result;

    // PE magic
    if (data[pe_offset] != 'P' || data[pe_offset + 1] != 'E') return result;

    uint16_t num_sections      = 0;
    uint16_t optional_hdr_size = 0;
    std::memcpy(&num_sections,      data + pe_offset +  6, sizeof(uint16_t));
    std::memcpy(&optional_hdr_size, data + pe_offset + 20, sizeof(uint16_t));

    uint32_t section_table_off = pe_offset + 24 + optional_hdr_size;

    fission::utils::log_stream() << "[DataSymbolRegistry] PE has " << num_sections
                                 << " sections (table offset 0x" << std::hex << section_table_off
                                 << ")" << std::dec << std::endl;

    for (uint16_t i = 0; i < num_sections && i < 64; ++i) {
        uint32_t entry_off = section_table_off + i * 40u;
        if (entry_off + 40u > size) break;

        char name_buf[9] = {};
        std::memcpy(name_buf, data + entry_off, 8);
        std::string sec_name(name_buf);

        // Only data sections we care about
        if (sec_name.find(".rdata") == std::string::npos &&
            sec_name.find(".data")  == std::string::npos) {
            continue;
        }

        uint32_t virtual_addr = 0;
        uint32_t raw_size     = 0;
        uint32_t raw_offset   = 0;
        std::memcpy(&virtual_addr, data + entry_off + 12, sizeof(uint32_t));
        std::memcpy(&raw_size,     data + entry_off + 16, sizeof(uint32_t));
        std::memcpy(&raw_offset,   data + entry_off + 20, sizeof(uint32_t));

        PeDataSection sec;
        sec.name        = sec_name;
        sec.va_addr     = image_base + virtual_addr;
        sec.file_offset = raw_offset;
        sec.file_size   = raw_size;
        result.push_back(sec);

        fission::utils::log_stream() << "[DataSymbolRegistry] Found data section: " << sec_name
                                     << " VA=0x" << std::hex << sec.va_addr
                                     << " file_offset=0x" << raw_offset
                                     << " size=" << std::dec << raw_size << std::endl;
    }

    return result;
}

// ---------------------------------------------------------------------------
// Unified scanner: PE parsing + DataSectionScanner + symbol registration
// ---------------------------------------------------------------------------

int scanAndRegisterDataSymbols(
    ghidra::Architecture*  arch,
    const uint8_t*         data,
    size_t                 size,
    uint64_t               image_base,
    const std::function<void(const fission::loaders::DataSymbol&)>& on_scanned_symbol
) {
    if (!arch || !data || size == 0) return 0;

    auto sections = extract_pe_data_sections(data, size, image_base);
    if (sections.empty()) {
        fission::utils::log_stream() << "[DataSymbolRegistry] No .rdata/.data sections found." << std::endl;
        return 0;
    }

    fission::loaders::DataSectionScanner scanner;
    int total = 0;

    for (const auto& sec : sections) {
        size_t start_idx = sec.file_offset;
        size_t end_idx   = start_idx + sec.file_size;

        if (end_idx > size) {
            fission::utils::log_stream() << "[DataSymbolRegistry] Warning: section '"
                                         << sec.name << "' extends beyond binary data." << std::endl;
            continue;
        }

        fission::utils::log_stream() << "[DataSymbolRegistry] Scanning section: " << sec.name
                                     << " at 0x" << std::hex << sec.va_addr
                                     << " size=" << std::dec << sec.file_size << std::endl;

        auto symbols = scanner.scanDataSection(data + start_idx, sec.va_addr, sec.file_size);
        total += registerDataSymbolsInGlobalScope(arch, symbols, on_scanned_symbol);
    }

    fission::utils::log_stream() << "[DataSymbolRegistry] scanAndRegisterDataSymbols: registered "
                                 << total << " symbols." << std::endl;
    return total;
}

// ---------------------------------------------------------------------------
// FFI path — uses pre-parsed memory_blocks from Rust loader
// ---------------------------------------------------------------------------

void registerDataSectionSymbols(fission::ffi::DecompContext* ctx) {
    fission::utils::log_stream() << "[DataSymbolRegistry] **CALLED** registerDataSectionSymbols" << std::endl;
    
    if (ctx == nullptr || !ctx->arch) {
        fission::utils::log_stream() << "[DataSymbolRegistry] ERROR: ctx or ctx->arch is null" << std::endl;
        return;
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] binary_data.size() = " << ctx->binary_data.size() << std::endl;
    fission::utils::log_stream() << "[DataSymbolRegistry] memory_blocks.size() = " << ctx->memory_blocks.size() << std::endl;
    
    Architecture* arch = ctx->arch.get();
    if (arch == nullptr) {
        fission::utils::log_stream() << "[DataSymbolRegistry] Missing required components" << std::endl;
        return;
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] Scanning data sections..." << std::endl;
    
    int totalSymbols = 0;
    DataSectionScanner scanner;
    
    // Scan each memory block that looks like data
    // D-1: Extended from ".rdata" / ".data" (PE) to include ELF and Mach-O read-only sections.
    static const std::unordered_set<std::string> data_sections = {
        ".rdata",        // PE read-only data
        ".data",         // PE writable data
        ".rodata",       // ELF read-only data
        "__const",       // Mach-O read-only constants
        ".data.rel.ro",  // ELF RELRO
    };

    for (const auto& block : ctx->memory_blocks) {
        // Only scan read-only data sections
        if (data_sections.count(block.name) == 0) {
            continue;
        }
        
        fission::utils::log_stream() << "[DataSymbolRegistry] Scanning section: " << block.name 
                  << " at 0x" << std::hex << block.va_addr 
                  << " size=" << std::dec << block.file_size << std::endl;
        
        // Check if we have the data
        size_t start_idx = block.file_offset;
        size_t end_idx = start_idx + block.file_size;
        
        if (end_idx > ctx->binary_data.size()) {
            fission::utils::log_stream() << "[DataSymbolRegistry] Warning: section extends beyond binary data" << std::endl;
            continue;
        }
        
        // Get pointer to section data
        const uint8_t* section_data = ctx->binary_data.data() + start_idx;
        
        // Scan for symbols
        std::vector<DataSymbol> symbols = scanner.scanDataSection(
            section_data,
            block.va_addr,
            block.file_size
        );
        
        totalSymbols += registerDataSymbolsInGlobalScope(arch, symbols);
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] Registered " << totalSymbols 
              << " data section symbols" << std::endl;
}

} // namespace core
} // namespace fission
