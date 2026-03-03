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
    result = SwitchReconstructor::reconstruct_switch_from_jump_table(result);
    result = SwitchReconstructor::reconstruct_switch_from_if_else_chain(result);
    result = SwitchReconstructor::reconstruct_switch_from_sequential_ifs(result);
    result = LabelAnalyzer::remove_unused_labels(result);
    
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

