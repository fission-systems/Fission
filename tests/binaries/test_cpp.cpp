#include <iostream>
#include <string>
#include <vector>

class Base {
public:
    virtual ~Base() {}
    virtual void say_hello() = 0;
    virtual int get_value() { return 42; }
};

class Derived : public Base {
private:
    std::string name;
    int value;

public:
    Derived(std::string n, int v) : name(n), value(v) {}
    
    void say_hello() override {
        std::cout << "Hello from " << name << "! Value: " << value << std::endl;
    }
    
    int get_value() override {
        return value * 2;
    }
};

void process_base(Base* b) {
    b->say_hello();
    std::cout << "Value: " << b->get_value() << std::endl;
}

int main() {
    Derived d("FissionTester", 1337);
    process_base(&d);
    return 0;
}
