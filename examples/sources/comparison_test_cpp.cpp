#include <cstdio>
#include <cstdlib>
#include <cstring>

struct Item {
    int id;
    char name[32];
    double value;
};

class Calculator {
public:
    virtual ~Calculator() = default;
    virtual int adjust(int x) const { return x + 1; }
};

class FancyCalculator : public Calculator {
public:
    int adjust(int x) const override {
        if ((x & 1) == 0) {
            return x * 2;
        }
        return x * 3;
    }
};

extern "C" int cpp_add(int a, int b) {
    return a + b;
}

extern "C" int cpp_switch(int x) {
    switch (x) {
    case 0: return 10;
    case 1: return 20;
    case 2: return 30;
    default: return x + 100;
    }
}

extern "C" int cpp_sum_array(const int* arr, int size) {
    int sum = 0;
    for (int i = 0; i < size; ++i) {
        sum += arr[i];
    }
    return sum;
}

extern "C" void cpp_init_item(Item* item, int id, const char* name, double value) {
    if (item == nullptr) {
        return;
    }
    item->id = id;
    std::strncpy(item->name, name, sizeof(item->name) - 1);
    item->name[sizeof(item->name) - 1] = '\0';
    item->value = value;
}

extern "C" Item* cpp_create_item(int id, const char* name, double value) {
    Item* item = static_cast<Item*>(std::malloc(sizeof(Item)));
    if (item != nullptr) {
        cpp_init_item(item, id, name, value);
    }
    return item;
}

extern "C" void cpp_destroy_item(Item* item) {
    if (item != nullptr) {
        std::free(item);
    }
}

extern "C" int cpp_virtual_compute(int x) {
    Calculator* calc = new FancyCalculator();
    int out = calc->adjust(x);
    delete calc;
    return out;
}

extern "C" int cpp_main_like() {
    std::puts("=== C++ decompiler benchmark ===");

    int arr[5] = {1, 2, 3, 4, 5};
    int sum = cpp_sum_array(arr, 5);

    Item* item = cpp_create_item(2026, "CppItem", 12.5);
    int score = cpp_add(sum, cpp_switch(2));
    score = cpp_add(score, cpp_virtual_compute(7));

    if (item != nullptr) {
        std::printf("Item %d %s %.2f\n", item->id, item->name, item->value);
        cpp_destroy_item(item);
    }

    return score;
}

int main() {
    return cpp_main_like();
}
