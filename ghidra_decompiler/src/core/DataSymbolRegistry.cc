#include "fission/loaders/DataSectionScanner.h"
#include "fission/core/ArchPolicy.h"
#include "fission/ffi/DecompContext.h"
#include "architecture.hh"
#include "database.hh"
#include "type.hh"
#include <iostream>
#include "fission/utils/logger.h"
#include <iomanip>

namespace fission {
namespace core {

using namespace ghidra;
using namespace fission::loaders;

/// \brief Register data section symbols in global scope
///
/// Scans data sections (.rdata, .data) for floating-point constants
/// and registers them as symbols in the global scope with proper types.
/// This enables type propagation through memory loads.
///
/// \param ctx Decompiler context with loaded binary
void registerDataSectionSymbols(fission::ffi::DecompContext* ctx) {
    fission::utils::log_stream() << "[DataSymbolRegistry] **CALLED** registerDataSectionSymbols" << std::endl;
    
    if (!ctx || !ctx->arch) {
        fission::utils::log_stream() << "[DataSymbolRegistry] ERROR: ctx or ctx->arch is null" << std::endl;
        return;
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] binary_data.size() = " << ctx->binary_data.size() << std::endl;
    fission::utils::log_stream() << "[DataSymbolRegistry] memory_blocks.size() = " << ctx->memory_blocks.size() << std::endl;
    
    Architecture* arch = ctx->arch.get();
    Scope* globalScope = arch->symboltab->getGlobalScope();
    TypeFactory* types = arch->types;
    AddrSpace* ramSpace = arch->getDefaultDataSpace();
    
    if (!globalScope || !types || !ramSpace) {
        fission::utils::log_stream() << "[DataSymbolRegistry] Missing required components" << std::endl;
        return;
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] Scanning data sections..." << std::endl;
    
    int totalSymbols = 0;
    DataSectionScanner scanner;
    
    // Scan each memory block that looks like data
    for (const auto& block : ctx->memory_blocks) {
        // Only scan read-only data sections
        if (block.name != ".rdata" && block.name != ".data") {
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
        
        // Register each symbol
        for (const auto& sym : symbols) {
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
                    // Create char array type: char[size]
                    Datatype* charType = types->getBase(1, TYPE_INT);  // char is 1-byte integer
                    if (charType) {
                        dt = types->getTypeArray(sym.size, charType);
                        fission::utils::log_stream() << "[DataSymbolRegistry] Creating char[" << sym.size 
                                  << "] type for string at 0x" << std::hex << sym.address << std::dec << std::endl;
                    }
                }
                
                if (!dt) {
                    fission::utils::log_stream() << "[DataSymbolRegistry] Could not create type for symbol at 0x" 
                              << std::hex << sym.address << std::endl;
                    continue;
                }
                
                // Create address
                Address addr(ramSpace, sym.address);
                
                // Check if symbol already exists
                SymbolEntry* existing = globalScope->queryContainer(addr, 1, Address());
                if (existing != nullptr) {
                    // Symbol already exists, optionally update type
                    Symbol* existingSym = existing->getSymbol();
                    if (existingSym && existingSym->getType()->getMetatype() == TYPE_UNKNOWN) {
                        // Update unknown type to our better type
                        fission::utils::log_stream() << "[DataSymbolRegistry] Updating type for existing symbol at 0x" 
                                  << std::hex << sym.address << std::endl;
                        // Note: Cannot directly update, Ghidra API limitation
                    }
                    continue;
                }
                
                // Add new symbol
                SymbolEntry* entry = globalScope->addSymbol(
                    sym.name,
                    dt,
                    addr,
                    Address()  // use point
                );
                
                if (entry) {
                    totalSymbols++;
                    fission::utils::log_stream() << "[DataSymbolRegistry] Registered symbol: " << sym.name 
                              << " at 0x" << std::hex << sym.address 
                              << " type=" << dt->getName() << std::endl;
                } else {
                    fission::utils::log_stream() << "[DataSymbolRegistry] Failed to add symbol at 0x" 
                              << std::hex << sym.address << std::endl;
                }
                
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[DataSymbolRegistry] Exception while registering symbol at 0x" 
                          << std::hex << sym.address << ": " << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[DataSymbolRegistry] Unknown exception while registering symbol at 0x" 
                          << std::hex << sym.address << std::endl;
            }
        }
    }
    
    fission::utils::log_stream() << "[DataSymbolRegistry] Registered " << totalSymbols 
              << " data section symbols" << std::endl;
}

} // namespace core
} // namespace fission
