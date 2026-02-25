/**
 * Fission Decompiler Post-Processing Pipeline
 */
#ifndef FISSION_DECOMPILER_POSTPROCESS_PIPELINE_H
#define FISSION_DECOMPILER_POSTPROCESS_PIPELINE_H

#include "fission/decompiler/AnalysisPipeline.h"
#include <string>

namespace fission {
namespace ffi {
struct DecompContext;
}

namespace decompiler {

struct PostProcessOptions {
    bool apply_struct_definitions = true;
    bool iat_symbols = true;
    bool strip_shadow_params = true;
    bool smart_constants = true;
    bool inline_strings = true;
    bool constants = true;
    bool guids = true;
    bool unicode_strings = true;
    bool interlocked_patterns = true;
    bool xunknown_types = true;
    bool seh_cleanup = true;
    bool global_symbols = true;
    bool internal_names = true;
    bool struct_offsets = true;
    bool fid_names = true;
};

std::string run_post_processing(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    const std::string& code,
    const AnalysisArtifacts& analysis,
    const PostProcessOptions& options
);

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_POSTPROCESS_PIPELINE_H
