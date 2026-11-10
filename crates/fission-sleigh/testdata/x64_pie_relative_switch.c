/* A switch on a character offset by a minimum, in position-independent code:
   the jump table holds offsets relative to its own base, and the selector is
   `c - 43`. Built with
     gcc -O0 -fPIE -nostdlib -nostartfiles -static-pie -Wl,-e,dispatch -o x64_pie_relative_switch.elf rel.c */
int dispatch(int c) {
    switch (c) {
    case '+': return 11;
    case ',': return 22;
    case '-': return 33;
    case '.': return 44;
    case '/': return 55;
    case '0': return 66;
    default:  return -1;
    }
}
