#include "fission/processing/PostProcessors.h"

#include <string>
#include <regex>

namespace fission {
namespace processing {

std::string standardize_variable_names(const std::string& code) {
    std::string result = code;

    std::regex stack_x_regex(R"(\b([a-z]+)?Stack([XY])_([0-9a-f]+)\b)", std::regex::icase);
    result = std::regex_replace(result, stack_x_regex, "local_$3");

    std::regex stack_regex(R"(\b([a-z]+)?Stack_([0-9a-f]+)\b)", std::regex::icase);
    result = std::regex_replace(result, stack_regex, "local_$2");

    return result;
}

std::string replace_xunknown_types(const std::string& code) {
    std::string result = code;

    std::regex xunknown_regex(R"(\bxunknown([1248])\b)");
    result = std::regex_replace(result, xunknown_regex, "undefined$1");

    std::regex uint4_regex(R"(\buint4\b)");
    result = std::regex_replace(result, uint4_regex, "uint");

    std::regex int4_regex(R"(\bint4\b)");
    result = std::regex_replace(result, int4_regex, "int");

    std::regex uint8_regex(R"(\buint8\b)");
    result = std::regex_replace(result, uint8_regex, "ulonglong");

    std::regex int8_regex(R"(\bint8\b)");
    result = std::regex_replace(result, int8_regex, "longlong");

    std::regex uint1_regex(R"(\buint1\b)");
    result = std::regex_replace(result, uint1_regex, "byte");

    std::regex uint2_regex(R"(\buint2\b)");
    result = std::regex_replace(result, uint2_regex, "ushort");

    std::regex int2_regex(R"(\bint2\b)");
    result = std::regex_replace(result, int2_regex, "short");

    std::regex unkbyte_regex(R"(\bunkbyte([0-9]+)\b)");
    result = std::regex_replace(result, unkbyte_regex, "undefined$1");

    std::regex unkint_regex(R"(\bunkint([0-9]+)\b)");
    result = std::regex_replace(result, unkint_regex, "undefined$1");

    std::regex float4_regex(R"(\bfloat4\b)");
    result = std::regex_replace(result, float4_regex, "float");

    std::regex float8_regex(R"(\bfloat8\b)");
    result = std::regex_replace(result, float8_regex, "double");

    std::regex float10_regex(R"(\bfloat10\b)");
    result = std::regex_replace(result, float10_regex, "long double");

    // ── Normalise undefined1/2/4/8 → standard-width integer types ──────────
    // Ghidra emits these both as type declarations and in cast expressions.
    // Map them to the corresponding unsigned fixed-width types so the output
    // reads more like production C code.
    //
    // Note: undefined alone (1-byte unresolved) maps to uint8_t as well.
    // Use word-boundary anchors to avoid false matches inside identifiers.
    static const std::regex undef8(R"(\bundefined8\b)");
    result = std::regex_replace(result, undef8, "uint64_t");

    static const std::regex undef4(R"(\bundefined4\b)");
    result = std::regex_replace(result, undef4, "uint32_t");

    static const std::regex undef2(R"(\bundefined2\b)");
    result = std::regex_replace(result, undef2, "uint16_t");

    static const std::regex undef1(R"(\bundefined1\b)");
    result = std::regex_replace(result, undef1, "uint8_t");

    // bare undefined (1-byte) — must come after numbered forms to avoid
    // matching a prefix of "undefined4" etc.
    static const std::regex undef_bare(R"(\bundefined\b)");
    result = std::regex_replace(result, undef_bare, "uint8_t");

    return result;
}

} // namespace processing
} // namespace fission
