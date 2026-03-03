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

    return result;
}

} // namespace processing
} // namespace fission
