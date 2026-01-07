/**
 * Fission Decompiler Analysis Pipeline
 *
 * Runs structure/type/global/stack analysis passes after initial decompilation.
 */
#ifndef FISSION_DECOMPILER_ANALYSIS_PIPELINE_H
#define FISSION_DECOMPILER_ANALYSIS_PIPELINE_H

#include <cstdint>
#include <map>
#include <string>

namespace ghidra {
class Action;
class Funcdata;
class TypeStruct;
}

namespace fission {
namespace ffi {
struct DecompContext;
}

namespace decompiler {

struct AnalysisArtifacts {
    std::string inferred_struct_definitions;
    std::map<unsigned long long, ghidra::TypeStruct*> captured_structs;
};

AnalysisArtifacts run_analysis_passes(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
);

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_ANALYSIS_PIPELINE_H
