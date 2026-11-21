/* Mach-O fixture: a function table, an imported call (stub + lazy pointer)
 * and a global, so the image carries LC_SYMTAB, LC_DYSYMTAB with an indirect
 * symbol table, LC_FUNCTION_STARTS and LC_MAIN.
 *
 *   clang -arch x86_64 -mmacosx-version-min=10.13 -O1 macho_calls.c \
 *       -o macho_x64_calls
 *   clang -target i386-apple-darwin -O1 -c macho_calls.c -o macho_x86_calls.o
 */
#include <stdio.h>

int counter = 7;

static int twice(int x) { return x * 2; }
int bump(int x) { counter += x; return twice(counter); }

int (*const table[])(int) = { bump, twice };

int main(int argc, char **argv) {
    printf("%d\n", table[argc & 1](argc));
    return puts(argv[0]) < 0;
}
