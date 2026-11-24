/*
 * Link-time stand-ins for the callees of `call_machines.c` in the corpus
 * ELF. Never run: the engine and its emulator oracle stub every call the
 * step functions make. They exist so the freestanding static link resolves
 * and the calls stay real `call` instructions to named functions -- the
 * names the signature library looks up.
 */

int isdigit(int c) { return c >= '0' && c <= '9'; }
int isalpha(int c) { return (c | 0x20) >= 'a' && (c | 0x20) <= 'z'; }
int putchar(int c) { return c & 0xff; }
void wbcq_log(unsigned event) { (void)event; }
__attribute__((noreturn)) void abort(void) { __builtin_trap(); }
