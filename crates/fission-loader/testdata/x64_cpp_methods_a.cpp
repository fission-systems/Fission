struct Base { int a; int tag; virtual int vget() const; int get(int k) const; };
int Base::vget() const { return a * 3; }
int Base::get(int k) const { return a + tag + k; }
struct Derived : Base { int b; int c; int sum(int k) const; };
int Derived::sum(int k) const { return a + b + c + get(k); }
static inline int helper(int x, int y) { return x * y + 1; }
int use(const Derived *d, int n) { return d->sum(n) + helper(n, d->b) + d->vget(); }
int main(int argc, char **argv) { Derived d; d.a = argc; d.b = 2; d.c = 3; d.tag = 4; return use(&d, argc); }
