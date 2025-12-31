#include "fission/loader/PeHeader.h"
#include <cstring>

namespace fission {
namespace loader {

PeDetectionResult detect_pe_arch(const std::vector<uint8_t>& bytes) {
    PeDetectionResult result = {}; // Zero-init all fields (is_pe=false, strings="", image_base=0)
    result.compiler_id = "default";
    
    if (bytes.size() < 0x40) return result;
    
    // Check DOS Signature "MZ"
    if (bytes[0] != 'M' || bytes[1] != 'Z') return result;
    
    // Get offset to PE header
    uint32_t pe_offset_raw = 0;
    std::memcpy(&pe_offset_raw, &bytes[0x3C], 4);
    uint64_t pe_offset = pe_offset_raw; // Promote to 64-bit to prevent overflow
    
    if (pe_offset + 4 > bytes.size()) return result;
    
    // Check PE Signature "PE\0\0"
    if (bytes[pe_offset] != 'P' || bytes[pe_offset+1] != 'E' || 
        bytes[pe_offset+2] != 0 || bytes[pe_offset+3] != 0) return result;
        
    result.is_pe = true;
    
    // Read Machine Type (offset + 4)
    // 0x14C = x86, 0x8664 = x64
    if (pe_offset + 6 > bytes.size()) return result;
    
    // Machine Type at PE+4
    uint16_t machine = 0;
    std::memcpy(&machine, &bytes[pe_offset + 4], 2);
    
    if (machine == 0x8664) {
        result.is_64bit = true;
    } else {
        result.is_64bit = false;
    }
    
    result.compiler_id = "windows";

    // Parse Optional Header to find Debug Directory
    // Optional Header starts at PE + 24
    size_t opt_header_offset = pe_offset + 24;
    // Magic: 0x10b (32-bit), 0x20b (64-bit)
    if (opt_header_offset + 2 > bytes.size()) return result;
    
    uint16_t magic = 0;
     std::memcpy(&magic, &bytes[opt_header_offset], 2);
    
    // Debug Dir RVA and Size offsets vary by magic
    // 32-bit: DataDirs at offset 96 from opt header start
    // 64-bit: DataDirs at offset 112 from opt header start
    // Debug entry is index 6 (size 8 bytes each: RVA, Size)
    
    size_t data_dirs_offset = opt_header_offset + (magic == 0x20b ? 112 : 96);
    size_t debug_dir_entry_offset = data_dirs_offset + (6 * 8); 
    
    if (debug_dir_entry_offset + 8 <= bytes.size()) {
       uint32_t debug_rva = 0;
       std::memcpy(&debug_rva, &bytes[debug_dir_entry_offset], 4);
       uint32_t debug_size = 0;
       std::memcpy(&debug_size, &bytes[debug_dir_entry_offset + 4], 4);
       
       if (debug_rva != 0 && debug_size != 0) {
           // Need to convert RVA to File Offset. Simple approach: iterate sections.
           // Size of Optional Header:
           uint16_t size_opt = 0;
           std::memcpy(&size_opt, &bytes[pe_offset + 20], 2);

           size_t section_table_offset = opt_header_offset + size_opt;
           uint16_t num_sections = 0;
           std::memcpy(&num_sections, &bytes[pe_offset + 6], 2);
           
           for (int i=0; i<num_sections; i++) {
               size_t section_entry = section_table_offset + (i * 40);
               if (section_entry + 40 > bytes.size()) break;
               
               uint32_t virt_addr = 0;
               std::memcpy(&virt_addr, &bytes[section_entry + 12], 4);
               uint32_t raw_size = 0;
               std::memcpy(&raw_size, &bytes[section_entry + 16], 4);
               uint32_t raw_ptr = 0;
               std::memcpy(&raw_ptr, &bytes[section_entry + 20], 4);
               
               if (debug_rva >= virt_addr && debug_rva < virt_addr + raw_size) {
                   uint32_t file_offset = raw_ptr + (debug_rva - virt_addr);
                   
                   // Parse Debug Directory Table
                   if (file_offset + debug_size <= bytes.size()) {
                       // Type 2 = CodeView
                       // Struct: Characteristics(4), TimeDate(4), Major(2), Minor(2), Type(4), SizeOfData(4), Address(4), Pointer(4)
                       // Type is at offset 12
                       uint32_t type = 0;
                       std::memcpy(&type, &bytes[file_offset + 12], 4);
                       
                       if (type == 2) {
                           uint32_t ptr_raw_data = 0;
                           std::memcpy(&ptr_raw_data, &bytes[file_offset + 24], 4);
                           
                           if (ptr_raw_data < bytes.size()) {
                               // CodeView Header: "RSDS" (0x53445352) + GUID(16) + Age(4) + Path
                               if (ptr_raw_data + 4 <= bytes.size() && 
                                   bytes[ptr_raw_data] == 'R' && bytes[ptr_raw_data+1] == 'S') {
                                   
                                   // PDB Path starts at offset 24 + ptr_raw_data
                                   size_t path_offset = ptr_raw_data + 24;
                                   if (path_offset < bytes.size()) {
                                       std::string pdb;
                                       while (path_offset < bytes.size() && bytes[path_offset] != 0) {
                                           pdb += (char)bytes[path_offset];
                                           path_offset++;
                                       }
                                       result.pdb_path = pdb;
                                   }
                               }
                           }
                       }
                   }
                   break;
               }
           }
       }
    }

    return result;
}

} // namespace loader
} // namespace fission
