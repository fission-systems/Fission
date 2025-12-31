/**
 * Fission Decompiler - Logging Utilities Implementation
 */

#include "fission/utils/logger.h"

namespace fission {
namespace utils {

static NullBuffer null_buffer;
static std::ostream null_stream_instance(&null_buffer);

std::ostream& null_stream() {
    return null_stream_instance;
}

} // namespace utils
} // namespace fission
