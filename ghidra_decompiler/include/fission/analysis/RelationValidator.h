#pragma once

#include <vector>
#include <map>
#include <string>
#include <cstdint>
#include <memory>
#include "fission/analysis/FidDatabase.h"

namespace fission {
namespace analysis {

/**
 * Validates FID matches by checking call graph relationships.
 * This mirrors Ghidra's Relation/Reference validation.
 */
class RelationValidator {
public:
    struct MatchResult {
        uint64_t function_id;
        std::string name;
        float confidence; // 0.0 to 1.0
        bool validated;
    };

    RelationValidator(std::shared_ptr<FidDatabase> db);
    ~RelationValidator();

    /**
     * Evaluate a candidate function match against actual call graph.
     * \param caller_id The FID Function ID from the database
     * \param actual_callee_hashes List of full hashes of functions called by this function in the binary
     * \return Confidence score (percentage of matched relations)
     */
    float evaluate_relations(uint64_t caller_id, const std::vector<uint64_t>& actual_callee_hashes);

    /**
     * Pick the best match among multiple candidates using relation validation.
     * \param actual_callee_hashes List of full hashes of functions called by this function in the binary.
     * \param actual_ref_strings Strings referenced near the function (LEA/MOV/PUSH etc.). Used for
     *        heuristic bonus (printf/fopen/assert patterns) to disambiguate hash collisions.
     * \param min_confidence_threshold Minimum score to mark as validated (default 0.3f).
     *        Use a higher value (e.g. 0.6f) for small functions to reduce false positives.
     */
    MatchResult find_best_match(const std::vector<const FidFunctionRecord*>& candidates,
                               const std::vector<uint64_t>& actual_callee_hashes,
                               const std::vector<std::string>& actual_ref_strings = {},
                               float min_confidence_threshold = 0.3f);

private:
    std::shared_ptr<FidDatabase> db;
};

} // namespace analysis
} // namespace fission
