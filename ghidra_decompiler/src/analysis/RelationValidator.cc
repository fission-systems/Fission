#include "fission/analysis/RelationValidator.h"
#include <algorithm>
#include <cctype>
#include <iostream>
#include <string>
#include "fission/utils/logger.h"

namespace fission {
namespace analysis {

namespace {

// Heuristic bonus when candidate function name + referenced strings match known CRT patterns.
// Disambiguates hash collisions (e.g. printf vs sprintf vs fprintf).
float string_ref_bonus(const std::string& candidate_name,
                       const std::vector<std::string>& actual_ref_strings) {
    if (actual_ref_strings.empty()) return 0.0f;

    auto lower = [](std::string s) {
        for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        return s;
    };
    const std::string name_lower = lower(candidate_name);

    // 1. Format-string functions: printf, sprintf, fprintf, swprintf, vprintf, vsprintf, ...
    const char* format_funcs[] = {
        "printf", "sprintf", "fprintf", "swprintf", "wprintf",
        "vprintf", "vsprintf", "vfprintf", "vswprintf", "vwprintf",
        "snprintf", "vsnprintf"
    };
    bool is_format_func = false;
    for (const char* f : format_funcs) {
        if (name_lower.find(f) != std::string::npos) {
            is_format_func = true;
            break;
        }
    }
    if (is_format_func) {
        for (const auto& s : actual_ref_strings) {
            if (s.find('%') != std::string::npos)
                return 0.3f;
        }
    }

    // 2. File-mode functions: fopen, _wfopen, popen, _popen, fopen_s, _wfopen_s
    const char* file_funcs[] = {
        "fopen", "_wfopen", "popen", "_popen", "fopen_s", "_wfopen_s"
    };
    static const char* file_modes[] = {
        "r", "w", "a", "rb", "wb", "ab", "rt", "wt", "at", "r+", "w+", "a+"
    };
    for (const char* f : file_funcs) {
        if (name_lower.find(f) != std::string::npos) {
            for (const auto& s : actual_ref_strings) {
                for (const char* m : file_modes) {
                    if (s == m) return 0.3f;
                }
            }
            break;
        }
    }

    // 3. Assert/error handlers: _assert, _wassert, __assert_fail, __crtMessageBox
    const char* assert_funcs[] = {
        "_assert", "_wassert", "__assert_fail", "__crtmessagebox", "assert"
    };
    for (const char* f : assert_funcs) {
        if (name_lower.find(f) != std::string::npos) {
            for (const auto& s : actual_ref_strings) {
                const std::string sl = lower(s);
                if (sl.find(".c") != std::string::npos || sl.find(".cpp") != std::string::npos ||
                    sl.find("expression:") != std::string::npos ||
                    sl.find("assertion failed") != std::string::npos ||
                    sl.find("assert") != std::string::npos)
                    return 0.4f;
            }
            break;
        }
    }

    return 0.0f;
}

} // anonymous namespace

RelationValidator::RelationValidator(std::shared_ptr<FidDatabase> db) : db(db) {}

RelationValidator::~RelationValidator() {}

float RelationValidator::evaluate_relations(uint64_t caller_id, const std::vector<uint64_t>& actual_callee_hashes) {
    // No callee info = cannot validate → return 0 (do not trust match)
    if (!db || actual_callee_hashes.empty()) {
        return 0.0f;
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

    // No valid callees checked = cannot validate → 0
    if (checked == 0) return 0.0f;
    
    float score = static_cast<float>(matched) / static_cast<float>(checked);
    
    // Debug logging
    // fission::utils::log_stream() << "[RelationValidator] Caller 0x" << std::hex << caller_id 
    //           << " matched " << std::dec << matched << "/" << checked 
    //           << " relations (score=" << score << ")" << std::endl;

    return score;
}

RelationValidator::MatchResult RelationValidator::find_best_match(
    const std::vector<const FidFunctionRecord*>& candidates,
    const std::vector<uint64_t>& actual_callee_hashes,
    const std::vector<std::string>& actual_ref_strings,
    float min_confidence_threshold)
{
    MatchResult best = {0, "", -0.1f, false};

    for (const auto* cand : candidates) {
        float relation_score = evaluate_relations(cand->function_id, actual_callee_hashes);
        float bonus = string_ref_bonus(cand->name, actual_ref_strings);
        float score = std::min(1.0f, relation_score + bonus);

        if (score >= best.confidence) {
            best.function_id = cand->function_id;
            best.name = cand->name;
            best.confidence = score;
            best.validated = (score >= min_confidence_threshold && score > 0.0f);
        }
    }

    // If best confidence is below threshold, explicitly mark as invalid
    if (best.confidence < min_confidence_threshold) {
        best.validated = false;
    }

    return best;
}

} // namespace analysis
} // namespace fission
