/* A small program with its own control flow, linked against the MinGW CRT,
 * for tests that run a Windows PE end to end:
 *
 *   - fission-emulator tests/windows_crt.rs: the CRT reaches `main` and exits,
 *     and `main`, `checksum` and `classify_range` run;
 *   - fission-dir tests/phase*: `count_bits` and `clamp` are decompiled and
 *     checked against what this machine code computes.
 *
 * It replaces `control_flow_gcc_O0.exe` / `control_flow_gcc-m32_O0.exe` from a
 * local dev corpus that no checkout carried, so those tests had never run.
 * Built -O0 with MinGW-w64 GCC 15.2.0 (Homebrew), from this directory:
 *
 *   x86_64-w64-mingw32-gcc -O0 -o ../x64_mingw_control_flow.exe mingw_control_flow.c
 *   i686-w64-mingw32-gcc   -O0 -o ../x86_mingw_control_flow.exe mingw_control_flow.c
 *   x86_64-w64-mingw32-strip --strip-debug ../x64_mingw_control_flow.exe
 *   i686-w64-mingw32-strip   --strip-debug ../x86_mingw_control_flow.exe
 *
 * Only debug sections are stripped: the tests find functions by their COFF
 * symbols. Run these only inside Fission.
 */

int count_bits(unsigned int x)
{
    int n = 0;
    while (x) {
        n += x & 1;
        x >>= 1;
    }
    return n;
}

int clamp(int v, int lo, int hi)
{
    if (v < lo)
        return lo;
    if (v > hi)
        return hi;
    return v;
}

unsigned int checksum(const char *s)
{
    unsigned int h = 0;
    while (*s)
        h = h * 31 + (unsigned char)*s++;
    return h;
}

int classify_range(int v)
{
    if (v < 0)
        return -1;
    if (v < 10)
        return 0;
    if (v < 100)
        return 1;
    return 2;
}

int main(int argc, char **argv)
{
    int total = count_bits(0xF0F0u) + clamp(argc, 0, 3) + classify_range(argc * 7);
    total += (int)(checksum(argv[0]) & 0xff);
    return total & 0x7f;
}
