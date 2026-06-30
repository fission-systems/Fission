#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Structure definitions for testing
typedef struct {
    int id;
    int value;
    char name[32];
} Record;

typedef struct Node {
    int data;
    struct Node *next;
    struct Node *prev;
} Node;

typedef struct {
    int x;
    int y;
    int z;
} Point3D;

// Function with struct parameter
int process_record(Record *rec) {
    if (rec == NULL) {
        return -1;
    }
    return rec->id + rec->value;
}

// Function that creates and manipulates structs
Record* create_record(int id, int value, const char *name) {
    Record *rec = (Record *)malloc(sizeof(Record));
    if (rec == NULL) {
        return NULL;
    }
    
    rec->id = id;
    rec->value = value;
    strncpy(rec->name, name, sizeof(rec->name) - 1);
    rec->name[sizeof(rec->name) - 1] = '\0';
    
    return rec;
}

// Linked list operations
Node* create_node(int data) {
    Node *node = (Node *)malloc(sizeof(Node));
    if (node != NULL) {
        node->data = data;
        node->next = NULL;
        node->prev = NULL;
    }
    return node;
}

// Insert node at beginning of list
Node* insert_at_head(Node *head, int data) {
    Node *new_node = create_node(data);
    if (new_node == NULL) {
        return head;
    }
    
    if (head != NULL) {
        new_node->next = head;
        head->prev = new_node;
    }
    
    return new_node;
}

// Sum all values in linked list
int sum_list(Node *head) {
    int sum = 0;
    Node *current = head;
    
    while (current != NULL) {
        sum += current->data;
        current = current->next;
    }
    
    return sum;
}

// Free linked list
void free_list(Node *head) {
    Node *current = head;
    while (current != NULL) {
        Node *next = current->next;
        free(current);
        current = next;
    }
}

// Function with array of pointers
int process_records(Record **records, int count) {
    int total = 0;
    
    for (int i = 0; i < count; i++) {
        if (records[i] != NULL) {
            total += records[i]->value;
        }
    }
    
    return total;
}

// Pointer arithmetic
int sum_array_via_pointers(int *arr, int len) {
    int sum = 0;
    int *ptr = arr;
    int *end = arr + len;
    
    while (ptr < end) {
        sum += *ptr;
        ptr++;
    }
    
    return sum;
}

// Double pointer manipulation
void modify_via_double_pointer(int **ptr_to_ptr, int value) {
    if (ptr_to_ptr != NULL && *ptr_to_ptr != NULL) {
        **ptr_to_ptr = value;
    }
}

// Main entry point
int main() {
    // Create records
    Record *rec1 = create_record(1, 100, "Alice");
    Record *rec2 = create_record(2, 200, "Bob");
    Record *rec3 = create_record(3, 300, "Charlie");
    
    // Array of record pointers
    Record *rec_array[] = {rec1, rec2, rec3};
    int total_records = process_records(rec_array, 3);
    
    // Linked list operations
    Node *list = NULL;
    list = insert_at_head(list, 10);
    list = insert_at_head(list, 20);
    list = insert_at_head(list, 30);
    list = insert_at_head(list, 40);
    
    int list_sum = sum_list(list);
    
    // Pointer arithmetic
    int arr[] = {1, 2, 3, 4, 5};
    int arr_sum = sum_array_via_pointers(arr, 5);
    
    // Double pointer
    int value = 42;
    int *ptr = &value;
    modify_via_double_pointer(&ptr, 100);
    
    // Cleanup
    free(rec1);
    free(rec2);
    free(rec3);
    free_list(list);
    
    printf("Total: %d %d %d\n", total_records, list_sum, arr_sum);
    
    return 0;
}
