/**
 * Graph Algorithms Implementation
 */

#include "fission/analysis/GraphAlgorithms.h"
#include <regex>
#include <stack>
#include <iostream>
#include <algorithm>
#include <sstream>

namespace fission {
namespace analysis {

// ============================================================================
// CFG Construction from Text
// ============================================================================

std::vector<Block> GraphAnalyzer::build_cfg_from_text(const std::string& c_code) {
    std::vector<Block> blocks;
    
    // 1. Identify Leaders (Block Starts): Line 1, Targets of Gotos, Instructions following Gotos/Returns
    std::vector<std::string> lines;
    std::stringstream ss(c_code);
    std::string line;
    while (std::getline(ss, line)) {
        lines.push_back(line);
    }
    
    // Map label name to line index
    std::map<std::string, int> label_to_line;
    // Lines that start a new block
    std::set<int> leaders;
    leaders.insert(0); // Entry

    std::regex label_pattern(R"(^\s*([A-Za-z_]\w*)\s*:(?!\s*:))");
    std::regex goto_pattern(R"(goto\s+([A-Za-z_]\w*)\s*;)");
    std::regex return_pattern(R"(\breturn\b)");

    for (int i = 0; i < lines.size(); ++i) {
        std::smatch match;
        
        // Label definition -> Leader
        if (std::regex_search(lines[i], match, label_pattern)) {
            label_to_line[match[1].str()] = i;
            leaders.insert(i);
        }
        
        // Control flow instruction -> Next line is leader
        bool is_goto = std::regex_search(lines[i], match, goto_pattern);
        bool is_ret = std::regex_search(lines[i], return_pattern);
        
        if (is_goto || is_ret) {
            if (i + 1 < lines.size()) {
                leaders.insert(i + 1);
            }
        }
    }

    if (lines.empty()) return blocks;

    // 2. Create Blocks
    auto it = leaders.begin();
    int current_start = *it;
    ++it;
    
    int block_id = 0;
    while (true) {
        int next_start = (it == leaders.end()) ? lines.size() : *it;
        
        Block block;
        block.id = block_id++;
        block.start_line = current_start;
        block.end_line = next_start - 1;
        
        // Check for label at start
        std::smatch match;
        if (std::regex_search(lines[current_start], match, label_pattern)) {
            block.label = match[1].str();
        }
        
        blocks.push_back(block);
        
        if (it == leaders.end()) break;
        current_start = *it;
        ++it;
    }

    // 3. Connect Edges
    for (auto& block : blocks) {
        std::string last_line = lines[block.end_line];
        std::smatch match;
        
        // Check for goto
        if (std::regex_search(last_line, match, goto_pattern)) {
            std::string target = match[1].str();
            if (label_to_line.count(target)) {
                int target_line = label_to_line[target];
                // Find block containing target_line
                for (const auto& other : blocks) {
                    if (other.start_line <= target_line && other.end_line >= target_line) {
                        block.succs.push_back(other.id);
                        break;
                    }
                }
            }
        } 
        
        // Check for return (no fallthrough)
        bool is_ret = std::regex_search(last_line, std::regex(R"(\breturn\b)"));
        
        // Fallthrough (unless unconditional goto or return)
        bool unconditional_goto = false;
         // Simple check: if line starts with goto (ignoring if)
        if (std::regex_match(last_line, std::regex(R"(^\s*goto\s+\w+\s*;)"))) {
            unconditional_goto = true;
        }

        if (!is_ret && !unconditional_goto) {
             // Fallthrough to next block (physically)
             if (block.id + 1 < blocks.size()) {
                 block.succs.push_back(block.id + 1);
             }
        }
    }
    
    // Fill preds
    for (const auto& b : blocks) {
        for (int succ : b.succs) {
            if (succ < blocks.size()) {
                blocks[succ].preds.push_back(b.id);
            }
        }
    }

    return blocks;
}

// ============================================================================
// Dominator Tree (Cooper, Harvey, Kennedy Algorithm)
// ============================================================================

std::map<int, int> GraphAnalyzer::compute_dominators(const std::vector<Block>& blocks) {
    std::map<int, int> doms; // id -> immediate dominator
    if (blocks.empty()) return doms;

    int start_node = 0;
    doms[start_node] = start_node; // Entry dominates itself

    bool changed = true;
    while (changed) {
        changed = false;
        
        // Reverse post-order traversal (using simple linear scan effectively works for reducible CFGs often, 
        // but robust implementation iterates all except start)
        for (const auto& block : blocks) {
            if (block.id == start_node) continue;

            int new_idom = -1;
            
            // First processed predecessor
            for (int pred : block.preds) {
                if (doms.count(pred)) {
                    new_idom = pred;
                    break;
                }
            }

            if (new_idom != -1) {
                for (int pred : block.preds) {
                    if (pred != new_idom && doms.count(pred)) {
                        // Intersect dominators
                        int finger1 = new_idom;
                        int finger2 = pred;
                        while (finger1 != finger2) {
                            while (finger1 > finger2) finger1 = doms[finger1];
                            while (finger2 > finger1) finger2 = doms[finger2];
                        }
                        new_idom = finger1;
                    }
                }

                if (doms.count(block.id) == 0 || doms[block.id] != new_idom) {
                    doms[block.id] = new_idom;
                    changed = true;
                }
            }
        }
    }
    
    return doms;
}

// ============================================================================
// Loop Detection
// ============================================================================

void GraphAnalyzer::find_natural_loop(int header, int latch, const std::vector<Block>& blocks, std::set<int>& loop_body) {
    loop_body.clear();
    loop_body.insert(header);
    
    std::stack<int> worklist;
    if (latch != header) {
        loop_body.insert(latch);
        worklist.push(latch);
    }
    
    while (!worklist.empty()) {
        int node = worklist.top();
        worklist.pop();
        
        for (int pred : blocks[node].preds) {
            if (loop_body.find(pred) == loop_body.end()) {
                loop_body.insert(pred);
                worklist.push(pred);
            }
        }
    }
}

std::vector<Loop> GraphAnalyzer::detect_loops(const std::vector<Block>& blocks) {
    std::vector<Loop> loops;
    if (blocks.empty()) return loops;
    
    // 1. Compute Dominators
    auto doms = compute_dominators(blocks);
    
    // 2. Find Back Edges (u -> v where v dominates u)
    std::map<int, std::vector<int>> header_to_latches;
    
    for (const auto& u : blocks) {
        for (int v : u.succs) {
            // Check domination: v must dominate u
            bool v_dominates_u = false;
            int curr = u.id;
            while (true) {
                if (curr == v) {
                    v_dominates_u = true;
                    break;
                }
                if (doms.count(curr) == 0 || doms.at(curr) == curr) break; // Reached root or undefined
                curr = doms.at(curr);
            }
            
            if (v_dominates_u) {
                header_to_latches[v].push_back(u.id);
            }
        }
    }
    
    // 3. Construct Natural Loops
    for (auto const& [header, latches] : header_to_latches) {
        Loop loop;
        loop.header_id = header;
        
        for (int latch : latches) {
            std::set<int> sub_body;
            find_natural_loop(header, latch, blocks, sub_body);
            loop.body.insert(sub_body.begin(), sub_body.end());
            loop.back_edges.push_back({latch, header});
        }
        
        loops.push_back(loop);
    }
    
    return loops;
}

} // namespace analysis
} // namespace fission
