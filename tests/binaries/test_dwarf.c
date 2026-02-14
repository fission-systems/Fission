#include <stdio.h>

typedef enum {
    TYPE_A,
    TYPE_B,
    TYPE_C
} EntryType;

struct Header {
    int magic;
    EntryType type;
};

struct Data {
    struct Header header;
    char name[32];
    double value;
};

void print_data(struct Data* d) {
    printf("Name: %s, Value: %f, Magic: %d\n", d->name, d->value, d->header.magic);
}

int main() {
    struct Data d = {{0x1234, TYPE_B}, "Test", 3.14};
    print_data(&d);
    return 0;
}
