/* Probe program for scripts/quality/support_matrix_probe.py. Small on purpose:
 * a loop, a switch, a byte scan, recursion, a string and a libc call. */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
static int fib(int n){ return n<2?n:fib(n-1)+fib(n-2); }
int sum_to(int n){ int s=0; for(int i=0;i<=n;i++) s+=i; return s; }
int classify(int x){ switch(x){case 0:return 10;case 1:return 20;case 2:return 35;case 3:return 47;case 4:return 59;case 5:return 61;default:return -1;} }
size_t count_char(const char*s,char c){ size_t n=0; while(*s){ if(*s==c) n++; s++; } return n; }
int main(int argc,char**argv){
  const char *msg="fission matrix probe";
  printf("%s %d %d %d %zu\n",msg,fib(argc+6),sum_to(argc*10),classify(argc),count_char(msg,'s'));
  return argc>5?1:0;
}
