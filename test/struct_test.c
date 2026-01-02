#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Define a simple structure to test Fission's structure recovery
typedef struct {
    int id;
    char name[32];
    double value;
    struct {
        int x;
        int y;
    } point;
} Item;

// Function that takes a pointer to a structure
// This should trigger Step 4b (Structure Recovery) in Fission
void process_item(Item* item) {
    if (item == NULL) return;
    
    printf("Processing Item ID: %d\n", item->id);
    printf("Name: %s\n", item->name);
    
    // Modify structure fields
    item->value *= 1.5;
    item->point.x += 10;
    item->point.y += 20;
    
    printf("New Value: %.2f\n", item->value);
    printf("New Position: (%d, %d)\n", item->point.x, item->point.y);
}

// Function with primitive types only (should NOT trigger Step 4b)
int add_numbers(int a, int b) {
    return a + b;
}

int main(int argc, char* argv[]) {
    printf("Fission Structure Recovery Test\n");
    
    Item* myItem = (Item*)malloc(sizeof(Item));
    if (!myItem) return 1;
    
    myItem->id = 1001;
    strcpy(myItem->name, "TestItem");
    myItem->value = 123.45;
    myItem->point.x = 5;
    myItem->point.y = 5;
    
    process_item(myItem);
    
    int sum = add_numbers(10, 20);
    printf("Sum: %d\n", sum);
    
    free(myItem);
    return 0;
}
