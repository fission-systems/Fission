#include <metal_stdlib>
using namespace metal;

// Exact, case-sensitive, nonempty byte literals. Return presence per pattern;
// no offsets are dropped by chunking because each input position has a thread.
kernel void literal_presence(
    device const uchar *input [[buffer(0)]],
    device const uchar *patterns [[buffer(1)]],
    device const uint2 *metadata [[buffer(2)]],
    device const uint2 *buckets [[buffer(3)]],
    device const uint *indices [[buffer(4)]],
    device atomic_uint *found [[buffer(5)]],
    constant uint &length [[buffer(6)]],
    uint position [[thread_position_in_grid]]) {
    if (position >= length) return;
    uint2 bucket = buckets[input[position]];
    for (uint i = 0; i < bucket.y; ++i) {
        uint id = indices[bucket.x + i];
        if (atomic_load_explicit(found + id, memory_order_relaxed)) continue;
        uint2 pattern = metadata[id];
        if (pattern.y > length - position) continue;
        bool matches = true;
        for (uint j = 1; j < pattern.y; ++j) {
            if (input[position + j] != patterns[pattern.x + j]) {
                matches = false;
                break;
            }
        }
        if (matches) atomic_store_explicit(found + id, 1u, memory_order_relaxed);
    }
}
