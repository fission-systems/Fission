#include "fission/loaders/DataSectionScanner.h"
#include "float.hh"
#include "translate.hh"
#include <cmath>
#include <sstream>
#include <iomanip>
#include <iostream>
#include "fission/utils/logger.h"

namespace fission {
namespace loaders {

DataSectionScanner::DataSectionScanner() {}
DataSectionScanner::~DataSectionScanner() {}

std::string DataSectionScanner::generateSymbolName(uint64_t address) {
    std::ostringstream ss;
    ss << "DAT_" << std::hex << std::setfill('0') << std::setw(8) << address;
    return ss.str();
}

bool DataSectionScanner::isReasonableValue(double value) {
    // Check for NaN, infinity
    if (std::isnan(value) || std::isinf(value)) {
        return false;
    }
    
    // Check for reasonable range
    // Most real-world constants are in this range
    if (value < -1e308 || value > 1e308) {
        return false;
    }
    
    // Values that are too close to zero but not exactly zero might be suspect
    // But we should allow denormalized numbers
    double abs_val = std::fabs(value);
    if (abs_val != 0.0 && abs_val < 1e-300) {
        return false;  // Suspiciously small
    }
    
    return true;
}

bool DataSectionScanner::looksLikeDouble(uint64_t bits) {
    // Use Ghidra's FloatFormat to decode
    ghidra::FloatFormat format(8);  // 8 bytes for double
    ghidra::FloatFormat::floatclass fclass;
    
    try {
        double value = format.getHostFloat(bits, &fclass);
        
        // Only accept normalized, denormalized, or zero
        // Reject NaN and infinity
        if (fclass == ghidra::FloatFormat::nan || 
            fclass == ghidra::FloatFormat::infinity) {
            return false;
        }
        
        // Check if value is reasonable
        if (!isReasonableValue(value)) {
            return false;
        }
        
        // Additional heuristic: check exponent and mantissa patterns
        // IEEE 754 double: 1 sign bit, 11 exponent bits, 52 mantissa bits
        uint64_t exponent = (bits >> 52) & 0x7FF;
        uint64_t mantissa = bits & 0xFFFFFFFFFFFFFULL;
        
        // All zeros is valid (0.0 or -0.0)
        if (exponent == 0 && mantissa == 0) {
            return true;
        }
        
        // All ones exponent is NaN/Inf (already rejected above)
        if (exponent == 0x7FF) {
            return false;
        }
        
        // Normalized numbers (exponent 1-2046)
        if (exponent >= 1 && exponent <= 2046) {
            return true;
        }
        
        // Denormalized numbers (exponent 0, mantissa non-zero)
        if (exponent == 0 && mantissa != 0) {
            // Only accept if value is in reasonable range
            return true;
        }
        
        return false;
    } catch (...) {
        return false;
    }
}

bool DataSectionScanner::looksLikeFloat(uint32_t bits) {
    // Use Ghidra's FloatFormat to decode
    ghidra::FloatFormat format(4);  // 4 bytes for float
    ghidra::FloatFormat::floatclass fclass;
    
    try {
        // Convert 32-bit to 64-bit for FloatFormat
        uint64_t bits64 = bits;
        double value = format.getHostFloat(bits64, &fclass);
        
        // Only accept normalized, denormalized, or zero
        if (fclass == ghidra::FloatFormat::nan || 
            fclass == ghidra::FloatFormat::infinity) {
            return false;
        }
        
        // Check if value is reasonable
        if (!isReasonableValue(value)) {
            return false;
        }
        
        // IEEE 754 float: 1 sign bit, 8 exponent bits, 23 mantissa bits
        uint32_t exponent = (bits >> 23) & 0xFF;
        uint32_t mantissa = bits & 0x7FFFFF;
        
        // All zeros is valid
        if (exponent == 0 && mantissa == 0) {
            return true;
        }
        
        // All ones exponent is NaN/Inf
        if (exponent == 0xFF) {
            return false;
        }
        
        // Normalized or denormalized
        return true;
    } catch (...) {
        return false;
    }
}

bool DataSectionScanner::looksLikeAsciiString(const uint8_t* data, size_t offset, size_t section_size, size_t& string_length) {
    // Check if data at offset looks like a null-terminated ASCII/UTF8 string
    // Minimum 4 characters for a meaningful string
    const size_t MIN_STRING_LEN = 4;
    const size_t MAX_STRING_LEN = 1024;  // Reasonable limit
    
    string_length = 0;
    size_t printable_count = 0;
    size_t i = offset;
    
    while (i < section_size && string_length < MAX_STRING_LEN) {
        uint8_t c = data[i];
        
        // Null terminator found
        if (c == 0) {
            break;
        }
        
        // Check if character is printable ASCII or common whitespace
        if ((c >= 32 && c <= 126) ||  // Printable ASCII
            c == '\n' || c == '\r' || c == '\t') {
            printable_count++;
        } else if (c < 32 || c >= 127) {
            // Non-printable character (except null terminator)
            // Allow a small number of these for UTF-8, but not too many
            if (printable_count == 0 || string_length - printable_count > 2) {
                return false;  // Too many non-printable chars
            }
        }
        
        string_length++;
        i++;
    }
    
    // Must have null terminator
    if (i >= section_size || data[i] != 0) {
        return false;
    }
    
    // Must be at least MIN_STRING_LEN characters
    if (string_length < MIN_STRING_LEN) {
        return false;
    }
    
    // At least 80% should be printable ASCII
    if (printable_count * 100 / string_length < 80) {
        return false;
    }
    
    return true;
}

std::vector<DataSymbol> DataSectionScanner::scanDataSection(
    const uint8_t* data,
    uint64_t section_va,
    size_t section_size
) {
    std::vector<DataSymbol> symbols;
    
    if (!data || section_size == 0) {
        return symbols;
    }
    
    fission::utils::log_stream() << "[DataSectionScanner] Scanning section at 0x" << std::hex 
              << section_va << " size=" << std::dec << section_size << std::endl;
    
    // FIRST PASS: Scan for strings
    // Strings are prioritized because they can be of variable length and
    // we need to avoid marking string data as floats/doubles
    fission::utils::log_stream() << "[DataSectionScanner] Pass 1: Scanning for strings..." << std::endl;
    for (size_t offset = 0; offset < section_size; ) {
        size_t string_length = 0;
        if (looksLikeAsciiString(data, offset, section_size, string_length)) {
            DataSymbol sym;
            sym.address = section_va + offset;
            sym.size = string_length + 1;  // Include null terminator
            sym.type_meta = 11;  // TYPE_ARRAY (will be char[])
            sym.type_id = "char";  // Character array type
            sym.name = generateSymbolName(sym.address);
            sym.raw_value = 0;  // Not applicable for strings
            
            symbols.push_back(sym);
            
            // Extract string for logging (truncate if too long)
            std::string str_preview;
            size_t preview_len = std::min(string_length, (size_t)60);
            for (size_t i = 0; i < preview_len; i++) {
                char c = data[offset + i];
                if (c >= 32 && c <= 126) {
                    str_preview += c;
                } else {
                    str_preview += '.';
                }
            }
            if (string_length > 60) {
                str_preview += "...";
            }
            
            fission::utils::log_stream() << "[DataSectionScanner] Found string at 0x" << std::hex 
                      << sym.address << " (len=" << std::dec << string_length 
                      << "): \"" << str_preview << "\"" << std::endl;
            
            // Skip past this string
            offset += string_length + 1;  // +1 for null terminator
        } else {
            offset++;
        }
    }
    
    // SECOND PASS: Scan for 8-byte aligned doubles
    fission::utils::log_stream() << "[DataSectionScanner] Pass 2: Scanning for doubles..." << std::endl;
    for (size_t offset = 0; offset + 8 <= section_size; offset += 8) {
        uint64_t addr = section_va + offset;
        
        // Skip if this overlaps with a string we already found
        bool overlap = false;
        for (const auto& sym : symbols) {
            if (addr >= sym.address && addr < sym.address + sym.size) {
                overlap = true;
                break;
            }
        }
        if (overlap) continue;
        
        uint64_t value64 = *reinterpret_cast<const uint64_t*>(data + offset);
        
        if (looksLikeDouble(value64)) {
            DataSymbol sym;
            sym.address = addr;
            sym.size = 8;
            sym.type_meta = 9;  // TYPE_FLOAT from type.hh
            sym.type_id = "float8";
            sym.name = generateSymbolName(sym.address);
            sym.raw_value = value64;
            
            symbols.push_back(sym);
            
            // Decode for logging
            ghidra::FloatFormat format(8);
            ghidra::FloatFormat::floatclass fclass;
            double dval = format.getHostFloat(value64, &fclass);
            
            fission::utils::log_stream() << "[DataSectionScanner] Found double at 0x" << std::hex 
                      << sym.address << ": " << std::dec << dval 
                      << " (0x" << std::hex << value64 << ")" << std::endl;
        }
    }
    
    // THIRD PASS: Scan for 4-byte aligned floats
    fission::utils::log_stream() << "[DataSectionScanner] Pass 3: Scanning for floats..." << std::endl;
    for (size_t offset = 0; offset + 4 <= section_size; offset += 4) {
        uint64_t addr = section_va + offset;
        
        // Skip if this overlaps with any symbol we already found
        bool overlap = false;
        for (const auto& sym : symbols) {
            if (addr >= sym.address && addr < sym.address + sym.size) {
                overlap = true;
                break;
            }
        }
        if (overlap) continue;
        
        uint32_t value32 = *reinterpret_cast<const uint32_t*>(data + offset);
        
        if (looksLikeFloat(value32)) {
            DataSymbol sym;
            sym.address = addr;
            sym.size = 4;
            sym.type_meta = 9;  // TYPE_FLOAT
            sym.type_id = "float4";
            sym.name = generateSymbolName(sym.address);
            sym.raw_value = value32;
            
            symbols.push_back(sym);
            
            // Decode for logging
            ghidra::FloatFormat format(4);
            ghidra::FloatFormat::floatclass fclass;
            double fval = format.getHostFloat(value32, &fclass);
            
            fission::utils::log_stream() << "[DataSectionScanner] Found float at 0x" << std::hex 
                      << sym.address << ": " << std::dec << fval 
                      << " (0x" << std::hex << value32 << ")" << std::endl;
        }
    }
    
    fission::utils::log_stream() << "[DataSectionScanner] Found total " << symbols.size() 
              << " data symbols (strings + floats/doubles)" << std::endl;
    
    return symbols;
}

} // namespace loaders
} // namespace fission
