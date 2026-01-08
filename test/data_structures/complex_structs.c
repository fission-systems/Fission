/**
 * Test: Complex Data Structures
 * Category: Data Structures
 * Difficulty: Medium-Hard
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Test 1: Nested structures
typedef struct {
    int x, y, z;
} Point3D;

typedef struct {
    Point3D start;
    Point3D end;
    double length;
} Line3D;

typedef struct {
    char name[32];
    Point3D position;
    int health;
    double score;
} Player;

// Test 2: Structure with union
typedef enum { TYPE_INT, TYPE_FLOAT, TYPE_STRING } ValueType;

typedef struct {
    ValueType type;
    union {
        int i;
        float f;
        char* s;
    } data;
} Variant;

// Test 3: Structure with function pointer
typedef int (*CompareFunc)(const void*, const void*);

typedef struct {
    void* data;
    int size;
    int capacity;
    CompareFunc compare;
} DynamicArray;

// Test 4: Linked list node
typedef struct ListNode {
    int value;
    struct ListNode* next;
    struct ListNode* prev;
} ListNode;

// Test 5: Binary tree with parent pointer
typedef struct TreeNode {
    int key;
    void* data;
    struct TreeNode* left;
    struct TreeNode* right;
    struct TreeNode* parent;
} TreeNode;

// Test 6: Complex nested structure
typedef struct {
    int id;
    char name[64];
    Point3D locations[10];
    int location_count;
    struct {
        int year;
        int month;
        int day;
    } created;
    struct {
        char author[32];
        int version;
    } metadata;
} ComplexRecord;

// Functions to test structure operations

void init_player(Player* p, const char* name, int x, int y, int z) {
    strncpy(p->name, name, 31);
    p->name[31] = '\0';
    p->position.x = x;
    p->position.y = y;
    p->position.z = z;
    p->health = 100;
    p->score = 0.0;
}

void print_player(const Player* p) {
    printf("Player: %s\n", p->name);
    printf("  Position: (%d, %d, %d)\n", 
           p->position.x, p->position.y, p->position.z);
    printf("  Health: %d, Score: %.2f\n", p->health, p->score);
}

Variant create_int_variant(int value) {
    Variant v;
    v.type = TYPE_INT;
    v.data.i = value;
    return v;
}

Variant create_float_variant(float value) {
    Variant v;
    v.type = TYPE_FLOAT;
    v.data.f = value;
    return v;
}

void print_variant(const Variant* v) {
    switch (v->type) {
        case TYPE_INT:
            printf("Int: %d\n", v->data.i);
            break;
        case TYPE_FLOAT:
            printf("Float: %.2f\n", v->data.f);
            break;
        case TYPE_STRING:
            printf("String: %s\n", v->data.s);
            break;
    }
}

ListNode* create_node(int value) {
    ListNode* node = (ListNode*)malloc(sizeof(ListNode));
    if (node) {
        node->value = value;
        node->next = NULL;
        node->prev = NULL;
    }
    return node;
}

void insert_after(ListNode* node, ListNode* new_node) {
    if (!node || !new_node) return;
    
    new_node->next = node->next;
    new_node->prev = node;
    
    if (node->next) {
        node->next->prev = new_node;
    }
    node->next = new_node;
}

void print_list(ListNode* head) {
    printf("List: ");
    while (head) {
        printf("%d ", head->value);
        head = head->next;
    }
    printf("\n");
}

ComplexRecord create_complex_record(int id, const char* name) {
    ComplexRecord rec;
    rec.id = id;
    strncpy(rec.name, name, 63);
    rec.name[63] = '\0';
    rec.location_count = 0;
    rec.created.year = 2024;
    rec.created.month = 1;
    rec.created.day = 8;
    strncpy(rec.metadata.author, "System", 31);
    rec.metadata.version = 1;
    return rec;
}

void add_location(ComplexRecord* rec, int x, int y, int z) {
    if (rec->location_count < 10) {
        rec->locations[rec->location_count].x = x;
        rec->locations[rec->location_count].y = y;
        rec->locations[rec->location_count].z = z;
        rec->location_count++;
    }
}

int main() {
    printf("=== Complex Structures Test ===\n\n");
    
    // Test 1: Nested structures
    Player player;
    init_player(&player, "Hero", 10, 20, 5);
    print_player(&player);
    
    // Test 2: Union variant
    Variant v1 = create_int_variant(42);
    Variant v2 = create_float_variant(3.14f);
    print_variant(&v1);
    print_variant(&v2);
    
    // Test 3: Linked list
    ListNode* head = create_node(1);
    ListNode* node2 = create_node(2);
    ListNode* node3 = create_node(3);
    insert_after(head, node2);
    insert_after(node2, node3);
    print_list(head);
    
    // Test 4: Complex nested record
    ComplexRecord rec = create_complex_record(1001, "TestRecord");
    add_location(&rec, 1, 2, 3);
    add_location(&rec, 4, 5, 6);
    printf("Record: %s (ID: %d)\n", rec.name, rec.id);
    printf("Locations: %d\n", rec.location_count);
    printf("Created: %04d-%02d-%02d\n", 
           rec.created.year, rec.created.month, rec.created.day);
    
    // Cleanup
    free(head);
    free(node2);
    free(node3);
    
    return 0;
}
