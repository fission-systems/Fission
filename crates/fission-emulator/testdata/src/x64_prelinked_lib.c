/* A shared object linked at a non-zero base, the way a prelinked library is:
 * its lowest PT_LOAD does not start at 0. `answer_ptr` needs an
 * R_X86_64_RELATIVE relocation, and `answer` is an exported function. */
int answer(void) { return 42; }
int (*answer_ptr)(void) = answer;
