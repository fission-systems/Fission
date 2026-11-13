/* Built with: zig cc -target riscv64-linux-musl -O1 -nostdlib -static -Wl,-e,pick -o riscv64_llvm_local_labels.elf riscv64_llvm_local_labels.c
 * LLVM keeps assembler-local `.L` labels and `$x` mapping symbols in the RISC-V symbol table. */
int classify(int x){ switch(x){case 0:return 10;case 1:return 20;case 2:return 35;case 3:return 47;case 4:return 59;case 5:return 61;default:return -1;} }
int sum_to(int n){ int s=0; for(int i=0;i<=n;i++) s+=i; return s; }
int pick(int a,int b){ return a<b ? classify(a) : sum_to(b); }
