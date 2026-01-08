/**
 * Test: C++ Features - Virtual Functions & Polymorphism
 * Category: C++ Features
 * Difficulty: Hard
 */

#include <cstdio>
#include <cstring>

// Test 1: Simple inheritance with virtual functions
class Shape {
protected:
    int x, y;
public:
    Shape(int px, int py) : x(px), y(py) {}
    virtual ~Shape() {}
    
    virtual double area() const = 0;  // Pure virtual
    virtual void draw() const {
        printf("Shape at (%d, %d)\n", x, y);
    }
    
    void move(int dx, int dy) {
        x += dx;
        y += dy;
    }
};

class Circle : public Shape {
private:
    double radius;
public:
    Circle(int px, int py, double r) : Shape(px, py), radius(r) {}
    
    double area() const override {
        return 3.14159 * radius * radius;
    }
    
    void draw() const override {
        printf("Circle at (%d, %d), radius: %.2f, area: %.2f\n", 
               x, y, radius, area());
    }
};

class Rectangle : public Shape {
private:
    int width, height;
public:
    Rectangle(int px, int py, int w, int h) 
        : Shape(px, py), width(w), height(h) {}
    
    double area() const override {
        return width * height;
    }
    
    void draw() const override {
        printf("Rectangle at (%d, %d), %dx%d, area: %.2f\n",
               x, y, width, height, area());
    }
};

// Test 2: Multiple inheritance
class Printable {
public:
    virtual void print() const = 0;
    virtual ~Printable() {}
};

class Serializable {
public:
    virtual void serialize(char* buffer, int size) const = 0;
    virtual ~Serializable() {}
};

class Document : public Printable, public Serializable {
private:
    char title[64];
    int page_count;
public:
    Document(const char* t, int pages) : page_count(pages) {
        strncpy(title, t, 63);
        title[63] = '\0';
    }
    
    void print() const override {
        printf("Document: %s (%d pages)\n", title, page_count);
    }
    
    void serialize(char* buffer, int size) const override {
        snprintf(buffer, size, "DOC:%s:%d", title, page_count);
    }
};

// Test 3: Virtual function in constructor/destructor context
class Base {
protected:
    int value;
public:
    Base(int v) : value(v) {
        printf("Base constructor: %d\n", value);
        init();  // Virtual call in constructor
    }
    
    virtual ~Base() {
        cleanup();  // Virtual call in destructor
        printf("Base destructor\n");
    }
    
    virtual void init() {
        printf("Base::init()\n");
    }
    
    virtual void cleanup() {
        printf("Base::cleanup()\n");
    }
    
    virtual void process() {
        printf("Base::process() value=%d\n", value);
    }
};

class Derived : public Base {
private:
    int extra;
public:
    Derived(int v, int e) : Base(v), extra(e) {
        printf("Derived constructor: %d, %d\n", value, extra);
    }
    
    ~Derived() override {
        printf("Derived destructor\n");
    }
    
    void init() override {
        printf("Derived::init()\n");
    }
    
    void cleanup() override {
        printf("Derived::cleanup()\n");
    }
    
    void process() override {
        printf("Derived::process() value=%d, extra=%d\n", value, extra);
    }
};

// Test 4: Function pointer to member function
class Calculator {
public:
    int add(int a, int b) { return a + b; }
    int multiply(int a, int b) { return a * b; }
    
    typedef int (Calculator::*Operation)(int, int);
    
    int compute(Operation op, int x, int y) {
        return (this->*op)(x, y);
    }
};

// Test functions
void test_polymorphism() {
    printf("\n=== Test 1: Polymorphism ===\n");
    
    Circle circle(10, 20, 5.0);
    Rectangle rect(30, 40, 10, 20);
    
    Shape* shapes[] = {&circle, &rect};
    
    for (int i = 0; i < 2; i++) {
        shapes[i]->draw();
        printf("Area: %.2f\n", shapes[i]->area());
    }
}

void test_multiple_inheritance() {
    printf("\n=== Test 2: Multiple Inheritance ===\n");
    
    Document doc("Test Report", 42);
    doc.print();
    
    char buffer[128];
    doc.serialize(buffer, sizeof(buffer));
    printf("Serialized: %s\n", buffer);
    
    Printable* p = &doc;
    p->print();
    
    Serializable* s = &doc;
    s->serialize(buffer, sizeof(buffer));
    printf("Via interface: %s\n", buffer);
}

void test_virtual_in_ctor_dtor() {
    printf("\n=== Test 3: Virtual in Constructor/Destructor ===\n");
    
    {
        Derived d(100, 200);
        d.process();
    }  // Destructor called here
}

void test_member_function_pointer() {
    printf("\n=== Test 4: Member Function Pointer ===\n");
    
    Calculator calc;
    Calculator::Operation op = &Calculator::add;
    printf("10 + 5 = %d\n", calc.compute(op, 10, 5));
    
    op = &Calculator::multiply;
    printf("10 * 5 = %d\n", calc.compute(op, 10, 5));
}

int main() {
    printf("=== C++ Virtual Functions Test ===\n");
    
    test_polymorphism();
    test_multiple_inheritance();
    test_virtual_in_ctor_dtor();
    test_member_function_pointer();
    
    return 0;
}
