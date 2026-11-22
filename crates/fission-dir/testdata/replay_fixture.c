// SPDX-License-Identifier: Apache-2.0
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((noinline, used)) uint32_t mix_word(uint32_t value)
{
    return value * 3u + 1u;
}

__attribute__((noinline, used)) uint32_t write_byte(uint8_t *out, uint32_t value)
{
    out[1] = UINT8_C(166);
    return value + 1u;
}

/* A diagnostic case that also reuses the full-width value after truncation. */
__attribute__((noinline, used)) uint32_t narrowing_store(uint8_t *out, uint32_t value)
{
    out[1] = (uint8_t)value;
    return value + 1u;
}

int main(int argc, char **argv)
{
    if (argc != 6) {
        return 2;
    }
    uint32_t input = (uint32_t)strtoull(argv[2], NULL, 10);
    uint8_t out[3];
    for (int i = 0; i < 3; i++) {
        out[i] = (uint8_t)strtoul(argv[i + 3], NULL, 10);
    }
    uint32_t result;
    if (strcmp(argv[1], "mix_word") == 0) {
        result = mix_word(input);
    } else if (strcmp(argv[1], "write_byte") == 0) {
        result = write_byte(out, input);
    } else if (strcmp(argv[1], "narrowing_store") == 0) {
        result = narrowing_store(out, input);
    } else {
        return 2;
    }
    printf("[%" PRIu32 ",[%u,%u,%u]]\n", result, out[0], out[1], out[2]);
    return 0;
}
