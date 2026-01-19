#include "fission/analysis/RelationValidator.h"
#include <algorithm>
#include <iostream>
#include "fission/utils/logger.h"

namespace fission {
namespace analysis {

RelationValidator::RelationValidator(std::shared_ptr<FidDatabase> db) : db(db) {}

RelationValidator::~RelationValidator() {}

float RelationValidator::evaluate_relations(uint64_t caller_id, const std::vector<uint64_t>& actual_callee_hashes) {
    if (!db || actual_callee_hashes.empty()) {
        return 0.5f; // Neutral confidence
    }

    int matched = 0;
    int checked = 0;

    for (uint64_t callee_hash : actual_callee_hashes) {
        if (callee_hash == 0) continue;
        
        checked++;
        if (db->has_relation(caller_id, callee_hash)) {
            matched++;
        }
    }

    if (checked > 0) {
         fission::utils::log_stream() << "[RelationValidator] Evaluated " << checked << " relations for caller " << std::hex << caller_id << ": matched " << std::dec << matched << std::endl;
    }

    if (checked == 0) return 0.5f;
    
    float score = static_cast<float>(matched) / static_cast<float>(checked);
    
    // Debug logging
    // fission::utils::log_stream() << "[RelationValidator] Caller 0x" << std::hex << caller_id 
    //           << " matched " << std::dec << matched << "/" << checked 
    //           << " relations (score=" << score << ")" << std::endl;

    return score;
}

RelationValidator::MatchResult RelationValidator::find_best_match(
    const std::vector<const FidFunctionRecord*>& candidates, 
    const std::vector<uint64_t>& actual_callee_hashes) 
{
    MatchResult best = {0, "", -0.1f, false};

    for (const auto* cand : candidates) {
        float score = evaluate_relations(cand->function_id, actual_callee_hashes);
        
        // If we have a clear match (score > 0), it's a strong signal.
        // If multiple candidates have score 0, we can't distinguish them via relations.
        if (score >= best.confidence) {
            best.function_id = cand->function_id;
            best.name = cand->name;
            best.confidence = score;
            best.validated = (score > 0.0f);
        }
    }

    return best;
}

} // namespace analysis
} // namespace fission
