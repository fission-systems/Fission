/**
 * test_structs_classes.cpp
 *
 * Tests decompiler type/struct/class recovery:
 *  - Plain C structs with field access
 *  - Nested structs
 *  - C++ classes with vtable (virtual dispatch)
 *  - Single/multiple inheritance
 *  - Constructor / destructor patterns
 *  - Member function calls (thiscall)
 *  - RTTI-related patterns
 *  - Struct arrays and linked lists
 */
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

// ---- Plain C struct with field offsets ----
struct Point {
    int x;       // offset 0
    int y;       // offset 4
};

struct Rect {
    Point origin;   // offset 0  (embedded struct)
    Point size;     // offset 8
};

Point make_point(int x, int y) {
    Point p;
    p.x = x;
    p.y = y;
    return p;
}

int rect_area(const Rect *r) {
    return r->size.x * r->size.y;
}

int rect_perimeter(const Rect *r) {
    return 2 * (r->size.x + r->size.y);
}

// ---- Nested struct / complex layout ----
struct FileHeader {
    uint32_t magic;      // offset 0
    uint16_t version;    // offset 4
    uint16_t flags;      // offset 6
    uint64_t timestamp;  // offset 8
    uint32_t data_size;  // offset 16
    uint32_t checksum;   // offset 20
    char     name[24];   // offset 24
};

int validate_header(const FileHeader *h) {
    if (h->magic != 0x46495353) return -1;  // "FISS"
    if (h->version < 1 || h->version > 10) return -2;
    if (h->data_size > 0x10000000) return -3;
    if (h->flags & 0x8000) return -4;
    return 0;
}

// ---- Linked list (pointer chasing) ----
struct ListNode {
    int value;
    ListNode *next;
};

ListNode* list_insert(ListNode *head, int value) {
    ListNode *node = (ListNode *)malloc(sizeof(ListNode));
    node->value = value;
    node->next = head;
    return node;
}

int list_length(const ListNode *head) {
    int count = 0;
    while (head != nullptr) {
        count++;
        head = head->next;
    }
    return count;
}

int list_sum(const ListNode *head) {
    int sum = 0;
    for (const ListNode *cur = head; cur != nullptr; cur = cur->next) {
        sum += cur->value;
    }
    return sum;
}

void list_free(ListNode *head) {
    while (head != nullptr) {
        ListNode *next = head->next;
        free(head);
        head = next;
    }
}

// ---- C++ class hierarchy (vtable) ----
class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;
    virtual const char* name() const = 0;
    virtual void describe() const {
        printf("%s: area = %.2f\n", name(), area());
    }
};

class Circle : public Shape {
    double radius;
public:
    explicit Circle(double r) : radius(r) {}
    double area() const override { return 3.14159265 * radius * radius; }
    const char* name() const override { return "Circle"; }
};

class Rectangle : public Shape {
    double width, height;
public:
    Rectangle(double w, double h) : width(w), height(h) {}
    double area() const override { return width * height; }
    const char* name() const override { return "Rectangle"; }
};

class Square : public Rectangle {
public:
    explicit Square(double side) : Rectangle(side, side) {}
    const char* name() const override { return "Square"; }
};

// ---- Multiple inheritance ----
class Drawable {
public:
    virtual ~Drawable() = default;
    virtual void draw() const {
        printf("[Drawable::draw]\n");
    }
    int z_order = 0;
};

class Serializable {
public:
    virtual ~Serializable() = default;
    virtual int serialize(char *buf, int max_len) const {
        return snprintf(buf, max_len, "{\"type\":\"unknown\"}");
    }
};

class Widget : public Drawable, public Serializable {
    char label[32];
    int x, y, w, h;
public:
    Widget(const char *lbl, int px, int py, int pw, int ph)
        : x(px), y(py), w(pw), h(ph) {
        strncpy(label, lbl, 31);
        label[31] = '\0';
    }
    void draw() const override {
        printf("[Widget '%s' at (%d,%d) %dx%d]\n", label, x, y, w, h);
    }
    int serialize(char *buf, int max_len) const override {
        return snprintf(buf, max_len,
            "{\"type\":\"widget\",\"label\":\"%s\",\"x\":%d,\"y\":%d}",
            label, x, y);
    }
};

// ---- Constructor / Destructor patterns ----
class Resource {
    char *data;
    size_t size;
public:
    Resource(size_t sz) : size(sz) {
        data = (char *)malloc(sz);
        if (data) memset(data, 0, sz);
    }
    ~Resource() {
        free(data);
        data = nullptr;
        size = 0;
    }
    void fill(char c) {
        if (data) memset(data, c, size);
    }
    size_t get_size() const { return size; }
};

// ---- Array of structs ----
struct Employee {
    int id;
    char name[32];
    double salary;
};

double total_salary(const Employee *emps, int count) {
    double total = 0;
    for (int i = 0; i < count; i++) {
        total += emps[i].salary;
    }
    return total;
}

Employee* find_employee(Employee *emps, int count, int id) {
    for (int i = 0; i < count; i++) {
        if (emps[i].id == id) {
            return &emps[i];
        }
    }
    return nullptr;
}

int main(int argc, char **argv) {
    // Struct tests
    Rect r = {{10, 20}, {100, 200}};
    printf("area = %d, perimeter = %d\n", rect_area(&r), rect_perimeter(&r));
    
    FileHeader h;
    h.magic = 0x46495353;
    h.version = 2;
    h.flags = 0;
    h.timestamp = 0;
    h.data_size = 1024;
    h.checksum = 0;
    strncpy(h.name, "test", sizeof(h.name));
    printf("validate: %d\n", validate_header(&h));
    
    // Linked list
    ListNode *list = nullptr;
    for (int i = 0; i < 10; i++) list = list_insert(list, i * 7);
    printf("list: len=%d, sum=%d\n", list_length(list), list_sum(list));
    list_free(list);
    
    // Virtual dispatch
    Shape *shapes[] = {
        new Circle(5.0),
        new Rectangle(3.0, 4.0),
        new Square(6.0)
    };
    for (int i = 0; i < 3; i++) {
        shapes[i]->describe();
        delete shapes[i];
    }
    
    // Multiple inheritance
    Widget w("OK", 10, 20, 80, 30);
    w.draw();
    char buf[256];
    w.serialize(buf, sizeof(buf));
    printf("serialized: %s\n", buf);
    
    // Constructor/destructor
    {
        Resource res(64);
        res.fill('A');
        printf("resource size: %zu\n", res.get_size());
    }  // destructor called here
    
    // Array of structs  
    Employee emps[] = {
        {1, "Alice", 50000.0},
        {2, "Bob",   60000.0},
        {3, "Carol", 55000.0}
    };
    printf("total salary: %.0f\n", total_salary(emps, 3));
    Employee *found = find_employee(emps, 3, 2);
    if (found) printf("found: %s (%.0f)\n", found->name, found->salary);
    
    return 0;
}
