#include "fission/analysis/FunctionMatcher.h"
#include "fission/utils/logger.h"
#include "fission/utils/json_utils.h"
#include <iostream>
#include <fstream>
#include <sstream>
#include <cstring>

namespace fission {
namespace analysis {

FunctionMatcher::FunctionMatcher() {
}

FunctionMatcher::~FunctionMatcher() {
}

void FunctionMatcher::load_builtin_msvc_x64() {
    // Common MSVC x64 CRT function prologues
    // These are simplified patterns for demonstration
    
    // malloc - typical pattern
    {
        FunctionSignature sig;
        sig.name = "malloc";
        sig.library = "ucrtbase";
        // sub rsp, XX; mov r8d, [rsp+XX] or similar
        sig.pattern = {0x48, 0x83, 0xEC};  // sub rsp, imm8
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Too generic, skip for now
    }
    
    // memcpy - MSVC x64
    {
        FunctionSignature sig;
        sig.name = "memcpy";
        sig.library = "ucrtbase";
        // mov r11, rsp; sub rsp, XX; push rbx
        sig.pattern = {0x4C, 0x8B, 0xDC, 0x48, 0x83, 0xEC};
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 6;
        signatures.push_back(sig);
    }
    
    // strlen - MSVC x64
    {
        FunctionSignature sig;
        sig.name = "strlen";
        sig.library = "ucrtbase";
        // mov rax, rcx; (48 8B C1)
        sig.pattern = {0x48, 0x8B, 0xC1};
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Too generic, skip
    }

    // fopen / fopen_s patterns
    {
        FunctionSignature sig;
        sig.name = "_fopen_s";
        sig.library = "ucrtbase";
        // Typical: mov [rsp+XX], rbx; push rdi
        sig.pattern = {0x48, 0x89, 0x5C, 0x24};
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 4;
        // Still generic, but useful
        signatures.push_back(sig);
    }
    
    // printf
    {
        FunctionSignature sig;
        sig.name = "printf";
        sig.library = "ucrtbase";
        // push rbp; mov rbp, rsp; sub rsp, XX
        sig.pattern = {0x48, 0x89, 0x4C, 0x24, 0x08};  // mov [rsp+8], rcx
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 5;
        signatures.push_back(sig);
    }
    
    // HeapAlloc wrapper
    {
        FunctionSignature sig;
        sig.name = "__acrt_heap_alloc";
        sig.library = "ucrtbase";
        // mov rax, qword ptr [rip+XX]
        sig.pattern = {0x48, 0x8B, 0x05};
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Very common, skip
    }

    // __acrt_iob_func(unsigned int index) -> FILE*
    // Called in -O2 CRT when printf decomposes to CRT internal calls.
    // Typical x64 prologue: sub rsp, 28h; mov ecx, ecx (normalize arg)
    // Pattern is too generic to be reliable by bytes alone; registered here
    // so the name-based resolver in TypePropagator knows the return semantics.
    {
        FunctionSignature sig;
        sig.name = "__acrt_iob_func";
        sig.library = "ucrtbase";
        sig.pattern = {};  // name-only: matched by symbol lookup, not bytes
        sig.mask = {};
        sig.pattern_length = 0;
        // Not pushed to signatures[] (byte pattern empty), but the entry
        // serves as documentation and allows future name-lookup tables.
    }

    // __stdio_common_vfprintf(options, stream, format, locale, arglist) -> int
    // Used in -O2 when printf inlines into ucrtbase CRT internal calls (x64).
    {
        FunctionSignature sig;
        sig.name = "__stdio_common_vfprintf";
        sig.library = "ucrtbase";
        sig.pattern = {};
        sig.mask = {};
        sig.pattern_length = 0;
    }

    fission::utils::log_stream() << "[FunctionMatcher] Loaded " << signatures.size() 
              << " built-in MSVC x64 signatures" << std::endl;
}

void FunctionMatcher::load_builtin_signatures(const std::string& platform) {
    signatures.clear();
    matched_funcs.clear();
    
    if (platform == "msvc_x64" || platform == "windows_x64") {
        load_builtin_msvc_x64();
    }
    // Add more platforms as needed
}

bool FunctionMatcher::load_signatures(const std::string& json_path) {
    std::ifstream file(json_path);
    if (!file.is_open()) {
        fission::utils::log_stream() << "[FunctionMatcher] Failed to open: " << json_path << std::endl;
        return false;
    }
    
    std::string content((std::istreambuf_iterator<char>(file)),
                         std::istreambuf_iterator<char>());
    
    // Parse JSON array of signature objects
    // Format: [{"name": "malloc", "pattern": "48 83 EC", "mask": "FF FF FF", "library": "ucrtbase"}, ...]
    
    auto parse_hex_bytes = [](const std::string& hex_str) -> std::vector<uint8_t> {
        std::vector<uint8_t> bytes;
        std::istringstream iss(hex_str);
        std::string token;
        while (iss >> token) {
            // Handle wildcards (e.g., "??" or "XX")
            if (token == "??" || token == "XX" || token == "xx") {
                bytes.push_back(0x00);
            } else {
                try {
                    bytes.push_back(static_cast<uint8_t>(std::stoul(token, nullptr, 16)));
                } catch (...) {
                    bytes.push_back(0x00);
                }
            }
        }
        return bytes;
    };
    
    auto objects = fission::utils::extract_json_array(content);
    
    int loaded_count = 0;
    for (const auto& obj : objects) {
        std::string name = fission::utils::extract_json_string(obj, "name");
        std::string pattern_str = fission::utils::extract_json_string(obj, "pattern");
        std::string mask_str = fission::utils::extract_json_string(obj, "mask");
        std::string library = fission::utils::extract_json_string(obj, "library");
        
        if (name.empty() || pattern_str.empty()) {
            continue;
        }
        
        FunctionSignature sig;
        sig.name = name;
        sig.library = library;
        sig.pattern = parse_hex_bytes(pattern_str);
        
        if (!mask_str.empty()) {
            sig.mask = parse_hex_bytes(mask_str);
        } else {
            // Default mask: all FF (exact match)
            sig.mask = std::vector<uint8_t>(sig.pattern.size(), 0xFF);
        }
        
        // Ensure mask and pattern have same length
        while (sig.mask.size() < sig.pattern.size()) {
            sig.mask.push_back(0xFF);
        }
        
        sig.pattern_length = static_cast<int>(sig.pattern.size());
        signatures.push_back(sig);
        loaded_count++;
    }
    
    fission::utils::log_stream() << "[FunctionMatcher] Loaded " << loaded_count 
              << " signatures from " << json_path << std::endl;
    return loaded_count > 0;
}

bool FunctionMatcher::match_pattern(const uint8_t* bytes, int size, 
                                    const FunctionSignature& sig) const {
    if (size < sig.pattern_length) return false;
    
    for (int i = 0; i < sig.pattern_length; ++i) {
        if (sig.mask[i] == 0x00) continue;  // Wildcard
        if ((bytes[i] & sig.mask[i]) != (sig.pattern[i] & sig.mask[i])) {
            return false;
        }
    }
    return true;
}

std::string FunctionMatcher::match(uint64_t address, const uint8_t* bytes, int size) {
    // Check cache first
    auto it = matched_funcs.find(address);
    if (it != matched_funcs.end()) {
        return it->second;
    }
    
    // Try each signature
    for (const auto& sig : signatures) {
        if (match_pattern(bytes, size, sig)) {
            matched_funcs[address] = sig.name;
            fission::utils::log_stream() << "[FunctionMatcher] Matched " << sig.name 
                      << " at 0x" << std::hex << address << std::dec << std::endl;
            return sig.name;
        }
    }
    
    return "";  // No match
}

std::string FunctionMatcher::match_by_fid(uint64_t address, const uint8_t* bytes, size_t size, bool is_x86) {
    static size_t debug_hash_count = 0;
    
    // Check cache first
    auto it = matched_funcs.find(address);
    if (it != matched_funcs.end()) {
        return it->second;
    }
    
    // Check if any FID databases are available
    if (fid_dbs_.empty() && (!fid_db || !fid_db->is_loaded())) {
        return "";
    }
    
    // Minimum function size for reliable matching
    if (size < 8) {
        return "";
    }

    // =========================================================================
    // GAP-3 FIX: Multi-tier hash matching (mirrors Ghidra's FidProgramSeeker)
    //
    // Ghidra chooses one of three hash tiers based on code_unit_size:
    //   Short  (<  5 code units): use full_hash + specific_hash mandatory
    //   Medium (< 30 code units): use full_hash, verify specific_hash
    //   Full   (>= 30 code units): full_hash alone is sufficient
    //
    // "code units" ~= number of instructions; 1 instr ~= 3-6 bytes on x86-64.
    // We approximate via byte size: short < ~20 B, medium < ~180 B.
    //
    // Specific-hash scoring prevents false-positives on tiny stub functions.
    // =========================================================================

    // Use all available bytes (up to 256) for a more accurate hash
    size_t hash_bytes = std::min(size, (size_t)256);
    uint64_t full_hash     = FidHasher::calculate_full_hash(bytes, hash_bytes);
    uint64_t specific_hash = FidHasher::calculate_specific_hash(bytes, std::min(size, (size_t)16));

    // Approximate code-unit count (Ghidra uses actual instruction count;
    // we estimate: x86 avg instruction size ~= 4 bytes)
    int approx_units = static_cast<int>(size / 4);
    bool need_specific_verify = (approx_units < 30);  // short + medium tiers
    bool need_strict_specific  = (approx_units <  5);  // short tier only

    // Debug: Print first 3 computed hashes
    if (debug_hash_count < 3) {
        fission::utils::log_stream() << "[FunctionMatcher] Computed hash at 0x" << std::hex << address
                  << ": full=0x" << full_hash << " specific=0x" << specific_hash
                  << " units~=" << std::dec << approx_units;
        fission::utils::log_stream() << " bytes=[";
        for (size_t i = 0; i < std::min(size, (size_t)8); ++i) {
            fission::utils::log_stream() << std::hex << (int)bytes[i] << " ";
        }
        fission::utils::log_stream() << "]" << std::dec << std::endl;
        debug_hash_count++;
    }
    
    // Search all loaded FID databases (multi-DB exhaustive lookup)
    const auto& dbs_to_search = fid_dbs_.empty()
        ? std::vector<const FidDatabase*>{fid_db}
        : fid_dbs_;

    // Best match: track highest score
    std::string best_name;
    int         best_score = -1;

    for (const auto* db : dbs_to_search) {
        if (!db || !db->is_loaded()) continue;

        // Use record-level lookup so we can access code_unit_size, specific_hash,
        // and relation data for scoring.
        std::vector<const FidFunctionRecord*> records =
            db->lookup_records_by_hash(full_hash);

        for (const FidFunctionRecord* rec : records) {
            if (!rec || rec->name.empty()) continue;

            int score = 10; // base score for full_hash match

            // ---- Tier-based specific hash verification -------------------
            if (need_specific_verify) {
                // Medium tier: specific hash must match or score drops
                if (rec->specific_hash != 0 && rec->specific_hash != specific_hash) {
                    if (need_strict_specific) {
                        // Short tier: must match — disqualify
                        continue;
                    }
                    // Medium tier: penalise but don't disqualify
                    score -= 5;
                } else {
                    score += 3; // bonus for specific hash match
                }
            }

            // ---- code_unit_size sanity: filter cross-tier collisions -----
            // If DB record says the function has e.g. 200 code units but our
            // function is only ~5 instructions, the hash is a false positive.
            if (rec->code_unit_size > 0) {
                int db_units = static_cast<int>(rec->code_unit_size);
                // Allow ±50% tolerance
                if (approx_units > 0) {
                    int ratio_pct = (approx_units * 100) / (db_units > 0 ? db_units : 1);
                    if (ratio_pct < 50 || ratio_pct > 200) {
                        score -= 7; // size mismatch penalty
                    }
                }
            }

            // ---- Child relation bonus ------------------------------------
            // If this function has a known callee relation (child), and the
            // callee matches something we've already identified, add bonus.
            // (Child = callee of the candidate function.)
            // Check via has_relation(rec->function_id, callee_full_hash).
            // We don't have callee hashes here, so skip for now; the
            // full implementation would look up matched callees from context.

            if (score > best_score) {
                best_score = score;
                best_name  = rec->name;
            }
        }
    }

    if (!best_name.empty() && best_score >= 0) {
        matched_funcs[address] = best_name;
        fission::utils::log_stream() << "[FunctionMatcher] FID MATCH! 0x" << std::hex << address
                  << " -> " << best_name
                  << " (score=" << std::dec << best_score
                  << ", hash=0x" << std::hex << full_hash << ")" << std::dec << std::endl;
        return best_name;
    }

    return "";  // No match across all DBs
}

} // namespace analysis
} // namespace fission
