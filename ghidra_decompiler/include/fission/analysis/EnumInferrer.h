#pragma once
/**
 * EnumInferrer — infers enum types from Pcode-level switch/comparison patterns.
 *
 * Scans a function's Pcode for a variable that is compared (INT_EQUAL /
 * INT_NOTEQUAL / BRANCHIND dispatch) against 3 or more distinct integer
 * constants.  When found, it creates a TypeEnum in Ghidra's type factory whose
 * named values correspond to those constants (e.g. case_0, case_1, …) and
 * applies the enum type to the Varnode so that PrintC can use it.
 *
 * Integration: call EnumInferrer::infer_and_apply(fd) after the standard
 * Ghidra analysis passes run and before the C text is generated.
 */

#include <string>
#include <map>
#include <cstdint>

// Forward declarations
namespace ghidra {
    class Funcdata;
    class TypeFactory;
    class Varnode;
    class TypeEnum;
}

namespace fission {
namespace analysis {

class EnumInferrer {
public:
    /**
     * Analyse @p fd and create/apply enum types where appropriate.
     * @return Number of enum types created.
     */
    static int infer_and_apply(ghidra::Funcdata* fd);

private:
    // Detect comparison-based enum candidates (INT_EQUAL / INT_NOTEQUAL chains)
    static int infer_from_comparisons(ghidra::Funcdata* fd,
                                      ghidra::TypeFactory* factory);

    // Detect jump-table based enum candidates (BRANCHIND)
    static int infer_from_jumptable(ghidra::Funcdata* fd,
                                    ghidra::TypeFactory* factory);

    // Create and apply a TypeEnum for a given variable and its case-value set
    static ghidra::TypeEnum* create_enum(
        ghidra::TypeFactory* factory,
        const std::string& base_name,
        const std::map<uint64_t, std::string>& values,
        int byte_size);
};

} // namespace analysis
} // namespace fission
