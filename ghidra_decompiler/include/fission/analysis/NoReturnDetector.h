#ifndef FISSION_NO_RETURN_DETECTOR_H
#define FISSION_NO_RETURN_DETECTOR_H

/**
 * NoReturnDetector — evidence-based no-return function discovery.
 *
 * Equivalent to Ghidra's FindNoReturnFunctionsAnalyzer:
 *   For each CALL instruction, if the containing basic block has no
 *   fall-through successor (the code after the CALL is unreachable),
 *   that counts as one evidence point for the called function being
 *   no-return.  Once a function accumulates >= k_threshold evidence
 *   points it is marked noreturn in the Ghidra Architecture's symbol
 *   scope, which then constrains later decompilation re-runs.
 *
 * Usage (per-function pass, called inside AnalysisPipeline):
 *
 *   NoReturnDetector detector;                     // shared across functions
 *   detector.collect_evidence(fd);                 // call for every Funcdata
 *   int marked = detector.apply(arch, threshold);  // mark confirmed funcs
 */

#include <cstdint>
#include <map>
#include <set>
#include <string>

namespace ghidra {
class Architecture;
class Funcdata;
}

namespace fission {
namespace analysis {

class NoReturnDetector {
public:
    /// Default evidence threshold: function must be called with no
    /// fall-through ≥ k_threshold times before we mark it noreturn.
    static constexpr int k_threshold = 3;

    NoReturnDetector() = default;
    ~NoReturnDetector() = default;

    /// Scan one decompiled function and accumulate call-site evidence.
    /// Should be called on every Funcdata before apply().
    void collect_evidence(ghidra::Funcdata* fd);

    /// After all functions have been scanned, mark suspected no-return
    /// functions in the architecture's symbol scope.
    ///
    /// @param arch     the live Ghidra architecture
    /// @param threshold override the default evidence threshold (0 = default)
    /// @return number of functions newly marked noreturn
    int apply(ghidra::Architecture* arch, int threshold = 0);

    /// Evidence count for a callee address (for diagnostics / testing).
    int evidence_count(uint64_t callee_addr) const;

    /// Already-confirmed no-return addresses (from apply()).
    const std::set<uint64_t>& confirmed() const { return confirmed_; }

    /// Reset all accumulated state.
    void reset();

private:
    // callee_address -> evidence count
    std::map<uint64_t, int> evidence_;

    // callee_address -> set of already confirmed
    std::set<uint64_t> confirmed_;

    /// Known static no-return names (mirrors Ghidra's NonReturningFunctionNames).
    static bool is_static_no_return(const std::string& name);
};

} // namespace analysis
} // namespace fission

#endif // FISSION_NO_RETURN_DETECTOR_H
