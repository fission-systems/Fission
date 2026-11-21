/* Relocatable-object fixture: calls, a data reference and a function
 * pointer table, so the object carries .rela.text and .rela.data.
 *
 *   clang -target x86_64-linux-gnu -c -O1 -fno-asynchronous-unwind-tables \
 *       reloc_object.c -o x64_reloc_object.o
 *   clang -target i386-linux-gnu -c -O1 -fno-asynchronous-unwind-tables \
 *       reloc_object.c -o x86_reloc_object.o
 */
extern int external_sink(int);
int counter = 7;

static int twice(int x) { return external_sink(x) * 2; }
int bump(int x) { counter += x; return twice(counter); }

int (*const table[])(int) = { bump, twice };
