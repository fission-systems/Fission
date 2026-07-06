#include <stdint.h>
#include <stddef.h>

typedef struct {
    int32_t x;
    int32_t y;
    uint8_t flags;
} PvPoint;

int pv_compare3(int value, int lo, int hi) {
    if (value < lo) {
        return lo - value;
    }
    if (value > hi) {
        return value - hi;
    }
    return 0;
}

uint32_t pv_pointer_sum(const uint8_t *data, size_t len) {
    uint32_t acc = 0;
    const uint8_t *cursor = data;
    for (size_t i = 0; i < len; i++) {
        acc += cursor[i] ^ (uint8_t)i;
    }
    return acc;
}

int pv_recursive_mix(int n) {
    if (n <= 1) {
        return n + 1;
    }
    return pv_recursive_mix(n - 1) + pv_recursive_mix(n - 2);
}

int pv_struct_access(PvPoint *points, size_t len) {
    int total = 0;
    for (size_t i = 0; i < len; i++) {
        if ((points[i].flags & 1u) != 0) {
            total += points[i].x;
        } else {
            total -= points[i].y;
        }
    }
    return total;
}

uint8_t pv_state_machine(const uint8_t *input, size_t len) {
    uint8_t state = 0;
    for (size_t i = 0; i < len; i++) {
        switch (state) {
            case 0:
                state = (input[i] == 0x41u) ? 1u : 0u;
                break;
            case 1:
                state = (input[i] == 0x42u) ? 2u : 0u;
                break;
            default:
                state ^= input[i];
                break;
        }
    }
    return state;
}
