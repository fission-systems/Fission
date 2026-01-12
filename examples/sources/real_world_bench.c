#include <stdio.h>
#include <stdint.h>
#include <string.h>

/* Real-world: Adler-32 checksum algorithm from zlib */
uint32_t adler32(uint32_t adler, const uint8_t *buf, size_t len) {
    uint32_t s1 = adler & 0xffff;
    uint32_t s2 = (adler >> 16) & 0xffff;
    size_t n;

    for (n = 0; n < len; n++) {
        s1 = (s1 + buf[n]) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    return (s2 << 16) | s1;
}

/* Real-world: A simplified MD5 transform-like function (complex bit manipulation) */
void complex_bit_manip(uint32_t *state, const uint8_t block[64]) {
    uint32_t a = state[0], b = state[1], c = state[2], d = state[3];
    uint32_t x[16];
    int i;

    for (i = 0; i < 16; i++) {
        x[i] = ((uint32_t)block[i * 4]) |
               (((uint32_t)block[i * 4 + 1]) << 8) |
               (((uint32_t)block[i * 4 + 2]) << 16) |
               (((uint32_t)block[i * 4 + 3]) << 24);
    }

    /* Round 1 */
    #define F(x, y, z) (((x) & (y)) | ((~x) & (z)))
    #define ROTATE_LEFT(x, n) (((x) << (n)) | ((x) >> (32 - (n))))
    
    for (i = 0; i < 16; i++) {
        uint32_t temp = d;
        d = c;
        c = b;
        b = b + ROTATE_LEFT((a + F(b, c, d) + x[i] + 0xd76aa478), 7);
        a = temp;
    }

    state[0] += a;
    state[1] += b;
    state[2] += c;
    state[3] += d;
}

/* Real-world: A simple state machine (typical for parsers) */
int simple_parser(const char *input) {
    int state = 0;
    int count = 0;
    while (*input) {
        switch (state) {
            case 0:
                if (*input == '<') state = 1;
                break;
            case 1:
                if (*input == '/') state = 2;
                else if (*input == '>') state = 0;
                else state = 3;
                break;
            case 2:
                if (*input == '>') state = 0;
                break;
            case 3:
                if (*input == '>') {
                    state = 0;
                    count++;
                }
                break;
        }
        input++;
    }
    return count;
}

int main(int argc, char **argv) {
    uint32_t adler = 1;
    uint8_t data[] = "Fission Decompiler Real World Test";
    uint32_t state[4] = {0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476};
    uint8_t block[64];
    memset(block, 0xAA, 64);

    uint32_t res1 = adler32(adler, data, sizeof(data));
    complex_bit_manip(state, block);
    int res2 = simple_parser("<html><body>Test</body></html>");

    printf("Results: %u, %08x, %d\n", res1, state[0], res2);
    return 0;
}
