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
};

} // namespace analysis
} // namespace fission

#endif // __EMULATION_ANALYZER_H__
