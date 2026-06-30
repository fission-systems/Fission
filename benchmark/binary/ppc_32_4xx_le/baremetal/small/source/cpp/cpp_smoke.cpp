typedef unsigned int u32;

volatile u32 cpp_sink = 0;

class Base {
public:
    virtual ~Base() {}
    virtual u32 compute(u32 x) = 0;
};

class Adder : public Base {
    u32 amount;
public:
    Adder(u32 a) : amount(a) {}
    u32 compute(u32 x) override {
        return x + amount;
    }
};

class Multiplier : public Base {
    u32 factor;
public:
    Multiplier(u32 f) : factor(f) {}
    u32 compute(u32 x) override {
        return x * factor;
    }
};

u32 process_base(Base& b, u32 val) {
    return b.compute(val);
}

u32 run_cpp_smoke(u32 seed) {
    Adder a(10);
    Multiplier m(5);
    
    u32 res1 = process_base(a, seed);
    u32 res2 = process_base(m, res1);
    
    cpp_sink = res2;
    return res2;
}

// Ensure the function is not mangled if we want to find it easily
extern "C" u32 c_entry_cpp_smoke(u32 seed) {
    return run_cpp_smoke(seed);
}
