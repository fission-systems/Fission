struct rec { int a; long b; short c; char d; double e; };
__attribute__((noinline)) long f(struct rec *r) { return r->b + r->c * 3 + r->d + (long)r->e + r->a; }
int main(int c, char **v) { struct rec r = {1,2,3,4,5.0}; return (int)f(&r); }
