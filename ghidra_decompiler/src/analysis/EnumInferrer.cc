#include "fission/analysis/EnumInferrer.h"
#include "fission/utils/logger.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"
#include "address.hh"

#include <map>
#include <set>
#include <vector>
#include <sstream>
#include <algorithm>

namespace fission {
namespace analysis {

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Resolve a Varnode back through COPY / CAST / ZEXT / SEXT to its logical root.
static ghidra::Varnode* resolve_root(ghidra::Varnode* vn, int depth = 4) {
    if (!vn || depth == 0 || !vn->isWritten()) return vn;
    ghidra::PcodeOp* def = vn->getDef();
    if (!def) return vn;
    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            return resolve_root(def->getIn(0), depth - 1);
        default:
            return vn;
    }
}

/// Returns true if @p vn is a function parameter or register-like input.
static bool is_enumerable_candidate(ghidra::Varnode* vn) {
    if (!vn) return false;
    // Prefer input parameters and small integers (≤8 bytes)
    return vn->getSize() <= 8 && !vn->isConstant() && !vn->isAnnotation();
}

/// Stable key for a Varnode (space + offset so we can group across ops).
static uint64_t varnode_key(const ghidra::Varnode* vn) {
    if (!vn) return 0;
    uint64_t space  = static_cast<uint64_t>(vn->getSpace()->getIndex()) & 0xff;
    uint64_t offset = vn->getOffset() & 0xFFFFFFFFFFFFFFULL;
    return (space << 56) | offset;
}

// ─── enum creation ────────────────────────────────────────────────────────────

ghidra::TypeEnum* EnumInferrer::create_enum(
    ghidra::TypeFactory* factory,
    const std::string& base_name,
    const std::map<uint64_t, std::string>& values,
    int byte_size)
{
    if (values.empty() || byte_size <= 0) return nullptr;

    // Clamp to valid enum sizes (1, 2, 4, 8)
    if (byte_size != 1 && byte_size != 2 && byte_size != 4 && byte_size != 8)
        byte_size = 4;

    // Avoid creating duplicate – return existing
    ghidra::Datatype* existing = factory->findByName(base_name);
    if (existing && existing->isEnumType())
        return dynamic_cast<ghidra::TypeEnum*>(existing);

    ghidra::TypeEnum* te = factory->getTypeEnum(base_name);
    if (!te) return nullptr;

    std::map<ghidra::uintb, std::string> nmap;
    for (auto const& [val, name] : values)
        nmap[static_cast<ghidra::uintb>(val)] = name;

    factory->setEnumValues(nmap, te);

    fission::utils::log_stream()
        << "[EnumInferrer] Created enum " << base_name
        << " (" << byte_size << " bytes, " << nmap.size() << " values)\n";
    return te;
}

// ─── comparison-based inference ───────────────────────────────────────────────

int EnumInferrer::infer_from_comparisons(ghidra::Funcdata* fd,
                                          ghidra::TypeFactory* factory)
{
    // Gather: varnode_key → { constant values } + first seen size
    struct CandidateInfo {
        std::set<uint64_t>   values;
        int                  byte_size = 4;
        ghidra::Varnode*     representative = nullptr;
    };
    std::map<uint64_t, CandidateInfo> candidates;

    auto it = fd->beginOpAll(), end = fd->endOpAll();
    for (; it != end; ++it) {
        ghidra::PcodeOp* op = it->second;
        if (!op || op->isDead()) continue;

        ghidra::OpCode code = op->code();
        if (code != ghidra::CPUI_INT_EQUAL &&
            code != ghidra::CPUI_INT_NOTEQUAL &&
            code != ghidra::CPUI_INT_LESS &&
            code != ghidra::CPUI_INT_LESSEQUAL &&
            code != ghidra::CPUI_INT_SLESS &&
            code != ghidra::CPUI_INT_SLESSEQUAL)
            continue;

        ghidra::Varnode* in0 = op->getIn(0);
        ghidra::Varnode* in1 = op->getIn(1);
        if (!in0 || !in1) continue;

        ghidra::Varnode* var_vn   = nullptr;
        ghidra::Varnode* const_vn = nullptr;

        if (in0->isConstant() && !in1->isConstant()) {
            const_vn = in0; var_vn = in1;
        } else if (in1->isConstant() && !in0->isConstant()) {
            const_vn = in1; var_vn = in0;
        } else {
            continue; // both constant or neither — skip
        }

        var_vn = resolve_root(var_vn);
        if (!is_enumerable_candidate(var_vn)) continue;

        uint64_t key = varnode_key(var_vn);
        auto& info = candidates[key];
        info.values.insert(const_vn->getOffset());
        info.byte_size = var_vn->getSize();
        if (!info.representative) info.representative = var_vn;
    }

    int created = 0;
    uint64_t func_addr = fd->getAddress().getOffset();

    for (auto& [key, info] : candidates) {
        // Require at least 3 distinct comparison constants to infer an enum
        if (info.values.size() < 3) continue;
        // Sanity: values should be in a reasonable range (0..65535 for most enums)
        uint64_t max_val = *info.values.rbegin();
        if (max_val > 0xFFFF) continue;

        // Build value→name map
        std::map<uint64_t, std::string> vmap;
        for (uint64_t v : info.values) {
            std::stringstream ns;
            ns << "case_" << v;
            vmap[v] = ns.str();
        }

        std::stringstream name_ss;
        name_ss << "enum_" << std::hex << func_addr << "_" << (key & 0xFFFFFF);
        std::string enum_name = name_ss.str();

        ghidra::TypeEnum* te = create_enum(factory, enum_name, vmap, info.byte_size);
        if (te && info.representative) {
            // Apply enum type to the varnode — updateType locks it
            info.representative->updateType(te, true, true);
            ++created;
        }
    }
    return created;
}

// ─── jump-table based inference ───────────────────────────────────────────────

int EnumInferrer::infer_from_jumptable(ghidra::Funcdata* fd,
                                        ghidra::TypeFactory* factory)
{
    // For jump tables resolved by Ghidra: the switch variable feeds a BRANCHIND.
    // We look for BRANCHIND ops whose input is (after resolving) a non-constant
    // Varnode; the number of out-edges of the block tells us the case count.
    int created = 0;
    uint64_t func_addr = fd->getAddress().getOffset();

    auto it = fd->beginOpAll(), end = fd->endOpAll();
    for (; it != end; ++it) {
        ghidra::PcodeOp* op = it->second;
        if (!op || op->isDead()) continue;
        if (op->code() != ghidra::CPUI_BRANCHIND) continue;

        ghidra::Varnode* target = op->getIn(0);
        if (!target) continue;
        ghidra::Varnode* root = resolve_root(target);
        if (!root || root->isConstant()) continue;
        if (!is_enumerable_candidate(root)) continue;

        // Count out-edges of the block containing this BRANCHIND
        ghidra::BlockBasic* bb = op->getParent();
        if (!bb) continue;
        int num_cases = bb->sizeOut();
        if (num_cases < 3 || num_cases > 512) continue; // sanity range

        // Build a dense enum 0..num_cases-1
        std::map<uint64_t, std::string> vmap;
        for (int i = 0; i < num_cases; ++i) {
            std::stringstream ns;
            ns << "case_" << i;
            vmap[static_cast<uint64_t>(i)] = ns.str();
        }

        std::stringstream name_ss;
        name_ss << "sw_enum_" << std::hex << func_addr
                << "_" << op->getAddr().getOffset();
        std::string enum_name = name_ss.str();

        ghidra::TypeEnum* te = create_enum(factory, enum_name, vmap, root->getSize());
        if (te) {
            root->updateType(te, true, true);
            ++created;
        }
    }
    return created;
}

// ─── public entry point ───────────────────────────────────────────────────────

int EnumInferrer::infer_and_apply(ghidra::Funcdata* fd) {
    if (!fd) return 0;
    ghidra::TypeFactory* factory = fd->getArch()->types;
    if (!factory) return 0;

    int n = 0;
    n += infer_from_comparisons(fd, factory);
    n += infer_from_jumptable(fd, factory);
    return n;
}

} // namespace analysis
} // namespace fission
