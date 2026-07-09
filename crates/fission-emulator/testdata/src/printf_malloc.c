/* Source for x64_dyn_printf_malloc.elf / x64_static_printf_malloc.elf
 *
 * Rebuild:
 *   zig cc -target x86_64-linux-musl -Os -dynamic -s \
 *     -o testdata/x64_dyn_printf_malloc.elf testdata/src/printf_malloc.c
 *   zig cc -target x86_64-linux-musl -O1 -static -s \
 *     -o testdata/x64_static_printf_malloc.elf testdata/src/printf_malloc.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

int main(void) {
    char *p = (char *)malloc(64);
    if (!p) {
        return 1;
    }
    memcpy(p, "hello", 6);
    size_t n = strlen(p);
    printf("msg=%s n=%d\n", p, (int)n);
    void *m = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (m == MAP_FAILED) {
        return 2;
    }
    ((char *)m)[0] = 'x';
    free(p);
    return 0;
}
