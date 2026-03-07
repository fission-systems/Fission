/**
 * CFG Structurizer Implementation
 * 
 * Main orchestrator for converting unstructured control flow (gotos) to structured constructs.
 * Delegates to specialized passes for specific transformations.
 * 
 * This is the simplified orchestrator after modularization into:
 * - cfg/LabelAnalyzer: Label and goto extraction/analysis
 * - cfg/GotoPatternMatcher: Forward/backward goto transformations
 * - cfg/LoopReconstructor: For/while loop reconstruction
 * - cfg/SwitchReconstructor: Switch statement reconstruction
 */

#include "fission/decompiler/CFGStructurizer.h"
#include "fission/decompiler/cfg/LabelAnalyzer.h"
#include "fission/decompiler/cfg/GotoPatternMatcher.h"
#include "fission/decompiler/cfg/LoopReconstructor.h"
#include "fission/decompiler/cfg/SwitchReconstructor.h"
#include "fission/utils/logger.h"
#include <string>

namespace fission {
namespace decompiler {

using cfg::LabelAnalyzer;
using cfg::GotoPatternMatcher;
using cfg::LoopReconstructor;
using cfg::SwitchReconstructor;

// Count net brace depth for a code string, skipping string/char literals and
// line comments.  Returns (open_count - close_count).
static int brace_balance(const std::string& code) {
    int depth = 0;
    bool in_str = false, in_char = false, in_lcomment = false;
    for (size_t i = 0; i < code.size(); ++i) {
        char c = code[i];
        // Line-comment: skip until newline.
        if (in_lcomment) {
            if (c == '\n') in_lcomment = false;
            continue;
        }
        // Detect // comment start (outside string/char).
        if (!in_str && !in_char && c == '/' && i + 1 < code.size() && code[i+1] == '/') {
            in_lcomment = true;
            continue;
        }
        // String literal toggle.
        if (!in_char && c == '"' && (i == 0 || code[i-1] != '\\')) {
            in_str = !in_str;
            continue;
        }
        // Char literal toggle.
        if (!in_str && c == '\'' && (i == 0 || code[i-1] != '\\')) {
            in_char = !in_char;
            continue;
        }
        if (!in_str && !in_char) {
            if (c == '{') depth++;
            else if (c == '}') depth--;
        }
    }
    return depth;
}

std::string CFGStructurizer::structurize(const std::string& c_code) {
    std::string result = c_code;

    int goto_count_before = 0;
    size_t pos = 0;
    while ((pos = result.find("goto ", pos)) != std::string::npos) {
        goto_count_before++;
        pos += 5;
    }

    // Apply transformations in order of specificity (most specific first)
    result = GotoPatternMatcher::flatten_nested_if_goto(result);
    result = LoopReconstructor::convert_for_loop_patterns(result);
    result = GotoPatternMatcher::convert_backward_gotos_to_loops(result);
    result = LoopReconstructor::convert_nested_loop_patterns(result);
    result = GotoPatternMatcher::convert_unconditional_backward_goto(result);
    result = LoopReconstructor::eliminate_loop_exits(result);
    result = LoopReconstructor::normalize_do_while_true(result);
    result = GotoPatternMatcher::eliminate_forward_gotos(result);
    result = SwitchReconstructor::reconstruct_switch_from_bounded_chain(result);
    result = SwitchReconstructor::reconstruct_switch_from_jump_table(result);
    result = SwitchReconstructor::reconstruct_switch_from_if_else_chain(result);
    result = SwitchReconstructor::reconstruct_switch_from_sequential_ifs(result);
    result = LabelAnalyzer::remove_unused_labels(result);

    // ── Brace-balance safety check ─────────────────────────────────────────
    // If any transform introduced unbalanced braces the output would be
    // syntactically broken.  Revert to the original Ghidra output so the
    // caller still receives valid (if less readable) code.
    int bal_before = brace_balance(c_code);
    int bal_after  = brace_balance(result);
    if (bal_before != bal_after) {
        fission::utils::log_stream() << "[CFGStructurizer] ERROR: brace imbalance detected after structurization "
                  << "(before=" << bal_before << ", after=" << bal_after << ") — "
                  << "reverting to original Ghidra output" << std::endl;
        return c_code;
    }

    int goto_count_after = 0;
    pos = 0;
    while ((pos = result.find("goto ", pos)) != std::string::npos) {
        goto_count_after++;
        pos += 5;
    }

    if (goto_count_before > goto_count_after) {
        fission::utils::log_stream() << "[CFGStructurizer] Eliminated " << (goto_count_before - goto_count_after)
                  << " gotos (" << goto_count_before << " -> " << goto_count_after << ")" << std::endl;
    }

    return result;
}

} // namespace decompiler
} // namespace fission

