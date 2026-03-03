#ifndef __EMULATION_ANALYZER_H__
#define __EMULATION_ANALYZER_H__

#include <map>
#include <vector>
#include <set>
#include <string>

// Ghidra includes
#include "funcdata.hh"
#include "block.hh"

namespace fission {
namespace analysis {

/// \brief Analyzes a function using lightweight emulation to tag meta-information
///
/// This analyzer walks the control-flow graph after decompilation, evaluates
/// conditions where possible, and injects [FISSION_META] comments into the
/// output to assist AI in understanding the code's runtime behavior.
class EmulationAnalyzer {
private:
    std::map<ghidra::Address, std::string> meta_tags;  ///< Collected meta-tags by address

    /// Evaluate a simple constant condition if possible
    bool try_evaluate_condition(ghidra::PcodeOp* cbranch_op, bool& result);

    /// GAP-5: Symbolic constant propagation — trace backwards through def-use
    /// chain (COPY, INT_ADD, INT_SUB, INT_AND, ZEXT, SEXT) up to `max_depth`
    /// hops.  Returns true if the Varnode can be resolved to a constant.
    /// \param vn      varnode to resolve
    /// \param out_val receives the constant value on success
    /// \param depth   current recursion depth (caller passes 0)
    bool try_propagate_constant(ghidra::Varnode* vn, ghidra::uintb& out_val,
                                int depth = 0);

    /// Set of addresses registered as function stubs during this pass
    std::set<ghidra::uintb> registered_callind_targets_;

public:
    EmulationAnalyzer();
    ~EmulationAnalyzer();

    /// Main analysis entry point
    /// \param fd is the function to analyze
    /// \return true if any meta-tags were generated
    bool analyze(ghidra::Funcdata* fd);

    /// Apply the gathered meta tags to the function (as comments)
    void apply_tags(ghidra::Funcdata* fd);
    
    /// Get collected tags for external use
    const std::map<ghidra::Address, std::string>& getTags() const { return meta_tags; }

    /// Get the set of CALLIND constant targets discovered during this pass
    const std::set<ghidra::uintb>& getResolvedCallinds() const
        { return registered_callind_targets_; }
};

} // namespace analysis
} // namespace fission

#endif // __EMULATION_ANALYZER_H__
