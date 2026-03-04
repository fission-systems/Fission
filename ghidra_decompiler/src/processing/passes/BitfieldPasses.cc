// BitfieldPasses.cc — post-processor pass that annotates bitfield extractions.
//
// Pattern detected:
//   dst = (src >> SHIFT) & MASK;               (right-shift then mask)
//   dst = (src & MASK) >> SHIFT;               (mask then right-shift)
//
// where MASK has exactly N consecutive 1-bits.  When 2 or more such
// extractions are found for the same `src` variable, the assignments are
// annotated with  bf[SHIFT:SHIFT+BITS-1]  comments; and a summary
// bitfield comment block is prepended to the function body.
//
// Output example:
//   // Bitfield layout of param_1:
//   //   bits 0..2   -> flags (3 bits)
//   //   bits 3..9   -> length (7 bits)
//   flags  = ( param_1 >> 0) & 0x7;   // bf[0:2]
//   length = ( param_1 >> 3) & 0x7f;  // bf[3:9]

#include "fission/processing/PostProcessors.h"
#include "fission/utils/logger.h"

#include <string>
#include <map>
#include <vector>
#include <regex>
#include <sstream>
#include <algorithm>
#include <cstdint>
#include <bitset>

namespace fission {
namespace processing {

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Return the number of consecutive 1-bits in @p mask (0 = not a field mask).
static int consecutive_ones(uint64_t mask) {
    if (mask == 0) return 0;
    // All 1-bits must be contiguous
    // Clear trailing zeros first
    while ((mask & 1) == 0) mask >>= 1;
    int count = 0;
    while (mask & 1) { ++count; mask >>= 1; }
    return (mask == 0) ? count : 0; // non-zero remainder → non-contiguous
}

/// Trim leading/trailing whitespace from @p s.
static std::string trim(const std::string& s) {
    auto b = s.find_first_not_of(" \t\r\n");
    if (b == std::string::npos) return "";
    auto e = s.find_last_not_of(" \t\r\n");
    return s.substr(b, e - b + 1);
}

// ─── main pass ───────────────────────────────────────────────────────────────

struct BitfieldExtract {
    std::string line;          // original line text
    size_t      line_start;    // byte offset in code string
    size_t      line_end;
    std::string dst_var;       // left-hand side variable name
    std::string src_var;       // source variable (the bit-field container)
    int         shift;         // right-shift amount
    uint64_t    mask;          // AND mask
    int         bits;          // number of field bits
    int         hi;            // MSB of field (shift + bits - 1)
};

std::string annotate_bitfield_extractions(const std::string& code) {
    if (code.size() > 512 * 1024) return code; // skip very large outputs

    // Pattern 1:  dst = (src >> SHIFT) & MASK ;
    // Pattern 2:  dst = (src & MASK) >> SHIFT ;   (less common but valid)
    // We match C identifiers and hex/decimal literals.
    static const std::regex pat1(
        R"((\b\w+\b)\s*=\s*\(\s*(\b\w+\b)\s*>>\s*(\d+)\s*\)\s*&\s*(0x[0-9a-fA-F]+|\d+)\s*;)",
        std::regex::optimize);
    static const std::regex pat2(
        R"((\b\w+\b)\s*=\s*\(\s*(\b\w+\b)\s*&\s*(0x[0-9a-fA-F]+|\d+)\s*\)\s*>>\s*(\d+)\s*;)",
        std::regex::optimize);

    std::vector<BitfieldExtract> extracts;

    // Walk line-by-line
    size_t pos = 0;
    while (pos < code.size()) {
        size_t nl = code.find('\n', pos);
        if (nl == std::string::npos) nl = code.size();
        std::string line = code.substr(pos, nl - pos);

        auto try_add = [&](const std::smatch& m, bool form2) {
            BitfieldExtract ex;
            ex.line       = line;
            ex.line_start = pos;
            ex.line_end   = nl;
            ex.dst_var    = form2 ? m[1].str() : m[1].str();
            ex.src_var    = form2 ? m[2].str() : m[2].str();
            std::string mask_s, shift_s;
            if (form2) { mask_s = m[3].str(); shift_s = m[4].str(); }
            else       { shift_s = m[3].str(); mask_s = m[4].str(); }
            ex.shift = std::stoi(shift_s);
            ex.mask  = std::stoull(mask_s, nullptr, 0);
            ex.bits  = consecutive_ones(ex.mask);
            ex.hi    = ex.shift + ex.bits - 1;
            if (ex.bits >= 1 && ex.bits < 32) {
                extracts.push_back(ex);
            }
        };

        std::smatch m1, m2;
        if (std::regex_search(line, m1, pat1)) try_add(m1, false);
        else if (std::regex_search(line, m2, pat2)) try_add(m2, true);

        pos = nl + 1;
    }

    // Group by src_var — only annotate if a src has 2+ extractions
    std::map<std::string, std::vector<size_t>> by_src;
    for (size_t i = 0; i < extracts.size(); ++i)
        by_src[extracts[i].src_var].push_back(i);

    // Determine which extract indices to annotate
    std::vector<bool> annotate(extracts.size(), false);
    for (auto& [src, indices] : by_src) {
        if (indices.size() < 2) continue;
        for (size_t idx : indices) annotate[idx] = true;
    }

    bool any = std::any_of(annotate.begin(), annotate.end(), [](bool b){ return b; });
    if (!any) return code; // nothing to do

    // Build inline-annotated version:
    // Replace each matched line with "LINE  /* bf[LO:HI] */"
    // Work right-to-left in the string to keep offsets valid.
    std::string result = code;
    // Compute per-line replacements sorted by line_start descending
    struct Replacement {
        size_t start, end;
        std::string new_line;
    };
    std::vector<Replacement> replacements;
    for (size_t i = 0; i < extracts.size(); ++i) {
        if (!annotate[i]) continue;
        const auto& ex = extracts[i];
        std::ostringstream oss;
        // Trim trailing semicolon from original line, then add annotation
        std::string trimmed = trim(ex.line);
        if (!trimmed.empty() && trimmed.back() == ';')
            trimmed.pop_back();
        oss << trimmed
            << ";  /* bf[" << ex.shift << ":" << ex.hi << "] */";
        replacements.push_back({ex.line_start, ex.line_end, oss.str()});
    }
    std::sort(replacements.begin(), replacements.end(),
        [](const Replacement& a, const Replacement& b){ return a.start > b.start; });
    for (auto& rep : replacements) {
        result.replace(rep.start, rep.end - rep.start, rep.new_line);
    }

    // Prepend summary comment block for each multi-extract src variable
    // before the first '{' of the function body
    std::ostringstream summary;
    bool summarized = false;
    for (auto& [src, indices] : by_src) {
        if (indices.size() < 2) continue;
        summarized = true;
        summary << "    // Bitfield layout of " << src << ":\n";
        // Sort by shift
        std::vector<size_t> sorted_idx = indices;
        std::sort(sorted_idx.begin(), sorted_idx.end(),
            [&](size_t a, size_t b){ return extracts[a].shift < extracts[b].shift; });
        for (size_t idx : sorted_idx) {
            const auto& ex = extracts[idx];
            summary << "    //   bits " << ex.shift << ".." << ex.hi
                    << "  -> " << ex.dst_var
                    << " (" << ex.bits << " bit" << (ex.bits != 1 ? "s" : "") << ")\n";
        }
    }
    if (summarized) {
        // Insert after the opening '{' of the function body
        size_t brace = result.find('{');
        if (brace != std::string::npos) {
            result.insert(brace + 1, "\n" + summary.str());
            fission::utils::log_stream()
                << "[BitfieldPasses] Annotated bitfield extractions\n";
        }
    }

    return result;
}

} // namespace processing
} // namespace fission
