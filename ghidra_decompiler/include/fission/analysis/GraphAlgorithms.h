/**
 * Graph Algorithms for Control Flow Analysis
 * 
 * Implements graph-theoretic algorithms like Dominators and Natural Loop Detection.
 * Designed to align with the Rust implementation in fission-analysis.
 */

#pragma once

#include <vector>
#include <set>
#include <map>
#include <string>

namespace fission {
namespace analysis {

// Simple CFG representation for analysis
struct Block {
    int id;
    int start_line;
    int end_line;
    std::string label; // Optional label at start
    std::vector<int> preds;
    std::vector<int> succs;
};

struct Loop {
    int header_id;
    std::set<int> body;
    std::set<int> exit_blocks;
    std::vector<std::pair<int, int>> back_edges;
    bool is_irreducible = false;
};

class GraphAnalyzer {
public:
    // Build a CFG from "flat" C code (with labels and gotos)
    static std::vector<Block> build_cfg_from_text(const std::string& c_code);

    // Compute Dominator Tree (returns map: block -> immediate dominator)
    static std::map<int, int> compute_dominators(const std::vector<Block>& blocks);

    // Detect Natural Loops using Tarjan/Havlak approach (Dominator-based)
    static std::vector<Loop> detect_loops(const std::vector<Block>& blocks);

private:
    static void find_natural_loop(int header, int latch, const std::vector<Block>& blocks, std::set<int>& loop_body);
};

} // namespace analysis
} // namespace fission
