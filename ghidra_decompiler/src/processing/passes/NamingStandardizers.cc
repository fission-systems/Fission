#include "fission/processing/PostProcessors.h"

#include <string>
#include <regex>

namespace fission {
namespace processing {

std::string standardize_variable_names(const std::string& code) {
    std::string result = code;

    static const std::regex stack_x_regex(R"(\b([a-z]+)?Stack([XY])_([0-9a-f]+)\b)", std::regex::icase | std::regex::optimize);
    result = std::regex_replace(result, stack_x_regex, "local_$3");

    static const std::regex stack_regex(R"(\b([a-z]+)?Stack_([0-9a-f]+)\b)", std::regex::icase | std::regex::optimize);
    result = std::regex_replace(result, stack_regex, "local_$2");

    return result;
}

std::string replace_xunknown_types(const std::string& code) {
    std::string result = code;

    static const std::regex xunknown_regex(R"(\bxunknown([1248])\b)", std::regex::optimize);
    result = std::regex_replace(result, xunknown_regex, "undefined$1");

    static const std::regex uint4_regex(R"(\buint4\b)", std::regex::optimize);
    result = std::regex_replace(result, uint4_regex, "uint");

    static const std::regex int4_regex(R"(\bint4\b)", std::regex::optimize);
    result = std::regex_replace(result, int4_regex, "int");

    static const std::regex uint8_regex(R"(\buint8\b)", std::regex::optimize);
    result = std::regex_replace(result, uint8_regex, "ulonglong");

    static const std::regex int8_regex(R"(\bint8\b)", std::regex::optimize);
    result = std::regex_replace(result, int8_regex, "longlong");

    static const std::regex uint1_regex(R"(\buint1\b)", std::regex::optimize);
    result = std::regex_replace(result, uint1_regex, "byte");

    static const std::regex uint2_regex(R"(\buint2\b)", std::regex::optimize);
    result = std::regex_replace(result, uint2_regex, "ushort");

    static const std::regex int2_regex(R"(\bint2\b)", std::regex::optimize);
    result = std::regex_replace(result, int2_regex, "short");

    static const std::regex unkbyte_regex(R"(\bunkbyte([0-9]+)\b)", std::regex::optimize);
    result = std::regex_replace(result, unkbyte_regex, "undefined$1");

    static const std::regex unkint_regex(R"(\bunkint([0-9]+)\b)", std::regex::optimize);
    result = std::regex_replace(result, unkint_regex, "undefined$1");

    static const std::regex float4_regex(R"(\bfloat4\b)", std::regex::optimize);
    result = std::regex_replace(result, float4_regex, "float");

    static const std::regex float8_regex(R"(\bfloat8\b)", std::regex::optimize);
    result = std::regex_replace(result, float8_regex, "double");

    static const std::regex float10_regex(R"(\bfloat10\b)", std::regex::optimize);
    result = std::regex_replace(result, float10_regex, "long double");

    // ── Normalise undefined1/2/4/8 → standard-width integer types ──────────
    static const std::regex undef8(R"(\bundefined8\b)", std::regex::optimize);
    result = std::regex_replace(result, undef8, "uint64_t");

    static const std::regex undef4(R"(\bundefined4\b)", std::regex::optimize);
    result = std::regex_replace(result, undef4, "uint32_t");

    static const std::regex undef2(R"(\bundefined2\b)", std::regex::optimize);
    result = std::regex_replace(result, undef2, "uint16_t");

    static const std::regex undef1(R"(\bundefined1\b)", std::regex::optimize);
    result = std::regex_replace(result, undef1, "uint8_t");

    // bare undefined (1-byte) — must come after numbered forms to avoid
    // matching a prefix of "undefined4" etc.
    static const std::regex undef_bare(R"(\bundefined\b)", std::regex::optimize);
    result = std::regex_replace(result, undef_bare, "uint8_t");

    return result;
}

} // namespace processing
} // namespace fission
