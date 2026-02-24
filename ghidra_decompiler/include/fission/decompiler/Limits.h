#ifndef FISSION_DECOMPILER_LIMITS_H
#define FISSION_DECOMPILER_LIMITS_H

#include <cstddef>

/**
 * @file Limits.h
 * @brief Central repository for analysis budget constants.
 *
 * B-2: Previously these values were scattered as file-local constexpr/literals
 * across DecompilationPipeline.cc, AnalysisPipeline.cpp, DecompilationCore.cpp
 * and several analysis files, making it easy for the copies to drift apart.
 * Define them once here; include this header wherever needed.
 */

namespace fission {
namespace decompiler {

/// Maximum function code size that the decompiler will analyse.
/// Functions larger than this are skipped to bound analysis time.
inline constexpr size_t k_max_function_size = 10'000;   // 10 KB

/// Maximum number of PTRSUB opcodes examined during struct recovery.
inline constexpr int k_max_ptrsub_ops = 100;

/// Look-ahead window for RIP-relative string reference collection (bytes).
inline constexpr size_t k_string_scan_window = 0x120;

/// followFlow upper bound relative to function start (bytes).
inline constexpr size_t k_follow_flow_limit = 0x2000;

/// callee relationship scan window inside a function body (bytes).
inline constexpr size_t k_callee_scan_window = 0x100;

/// Maximum recursion depth for allocator-flow detection.
inline constexpr int k_allocator_flow_depth = 6;

/// Maximum prologue-pattern recognition iterations per address.
/// Used to bound the unified prologue scan on very large binaries.
inline constexpr size_t k_max_prologue_candidates = 500'000;

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_LIMITS_H
