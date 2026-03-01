/**
 * test_real_world_algorithms.cpp
 *
 * Tests decompiler quality on real-world algorithm patterns:
 *  - Sorting algorithms (qsort, insertion, merge)
 *  - Binary search
 *  - Hash table (open addressing)
 *  - Ring buffer / circular queue
 *  - Base64 encode/decode
 *  - LRU-style eviction
 *  - Command parser (real application pattern)
 */
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

// ==== Sorting ====

// Insertion sort (simple loop, array indexing)
void insertion_sort(int *arr, int n) {
    for (int i = 1; i < n; i++) {
        int key = arr[i];
        int j = i - 1;
        while (j >= 0 && arr[j] > key) {
            arr[j + 1] = arr[j];
            j--;
        }
        arr[j + 1] = key;
    }
}

// Partition for quicksort
static int partition(int *arr, int low, int high) {
    int pivot = arr[high];
    int i = low - 1;
    for (int j = low; j < high; j++) {
        if (arr[j] <= pivot) {
            i++;
            int tmp = arr[i]; arr[i] = arr[j]; arr[j] = tmp;
        }
    }
    int tmp = arr[i + 1]; arr[i + 1] = arr[high]; arr[high] = tmp;
    return i + 1;
}

void quicksort(int *arr, int low, int high) {
    if (low < high) {
        int pi = partition(arr, low, high);
        quicksort(arr, low, pi - 1);
        quicksort(arr, pi + 1, high);
    }
}

// ==== Binary Search ====

int binary_search(const int *arr, int n, int target) {
    int lo = 0, hi = n - 1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        if (arr[mid] == target) return mid;
        if (arr[mid] < target) lo = mid + 1;
        else hi = mid - 1;
    }
    return -1;
}

// ==== Hash Table (open addressing, linear probing) ====

#define HT_SIZE 64

struct HashEntry {
    char key[32];
    int value;
    int occupied;
};

struct HashTable {
    HashEntry entries[HT_SIZE];
};

static uint32_t ht_hash(const char *key) {
    uint32_t h = 5381;
    while (*key) {
        h = ((h << 5) + h) + (uint8_t)*key;
        key++;
    }
    return h;
}

void ht_init(HashTable *ht) {
    memset(ht, 0, sizeof(HashTable));
}

int ht_put(HashTable *ht, const char *key, int value) {
    uint32_t idx = ht_hash(key) % HT_SIZE;
    for (int i = 0; i < HT_SIZE; i++) {
        uint32_t probe = (idx + i) % HT_SIZE;
        if (!ht->entries[probe].occupied ||
            strcmp(ht->entries[probe].key, key) == 0) {
            strncpy(ht->entries[probe].key, key, 31);
            ht->entries[probe].key[31] = '\0';
            ht->entries[probe].value = value;
            ht->entries[probe].occupied = 1;
            return 0;
        }
    }
    return -1;  // full
}

int ht_get(const HashTable *ht, const char *key, int *out_value) {
    uint32_t idx = ht_hash(key) % HT_SIZE;
    for (int i = 0; i < HT_SIZE; i++) {
        uint32_t probe = (idx + i) % HT_SIZE;
        if (!ht->entries[probe].occupied) return -1;
        if (strcmp(ht->entries[probe].key, key) == 0) {
            *out_value = ht->entries[probe].value;
            return 0;
        }
    }
    return -1;
}

// ==== Ring Buffer ====

struct RingBuffer {
    uint8_t *data;
    int capacity;
    int head;
    int tail;
    int count;
};

RingBuffer* rb_create(int capacity) {
    RingBuffer *rb = (RingBuffer *)malloc(sizeof(RingBuffer));
    rb->data = (uint8_t *)malloc(capacity);
    rb->capacity = capacity;
    rb->head = 0;
    rb->tail = 0;
    rb->count = 0;
    return rb;
}

int rb_push(RingBuffer *rb, uint8_t byte) {
    if (rb->count >= rb->capacity) return -1;
    rb->data[rb->tail] = byte;
    rb->tail = (rb->tail + 1) % rb->capacity;
    rb->count++;
    return 0;
}

int rb_pop(RingBuffer *rb, uint8_t *out) {
    if (rb->count <= 0) return -1;
    *out = rb->data[rb->head];
    rb->head = (rb->head + 1) % rb->capacity;
    rb->count--;
    return 0;
}

void rb_free(RingBuffer *rb) {
    free(rb->data);
    free(rb);
}

// ==== Base64 Encode ====

static const char b64_table[] =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

int base64_encode(const uint8_t *input, int input_len, char *output, int output_max) {
    int out_len = 0;
    int i;
    for (i = 0; i + 2 < input_len; i += 3) {
        if (out_len + 4 >= output_max) return -1;
        uint32_t triple = ((uint32_t)input[i] << 16) |
                          ((uint32_t)input[i+1] << 8) |
                          (uint32_t)input[i+2];
        output[out_len++] = b64_table[(triple >> 18) & 0x3F];
        output[out_len++] = b64_table[(triple >> 12) & 0x3F];
        output[out_len++] = b64_table[(triple >> 6) & 0x3F];
        output[out_len++] = b64_table[triple & 0x3F];
    }
    // Handle remaining bytes
    if (i < input_len) {
        if (out_len + 4 >= output_max) return -1;
        uint32_t triple = (uint32_t)input[i] << 16;
        if (i + 1 < input_len) triple |= (uint32_t)input[i+1] << 8;
        output[out_len++] = b64_table[(triple >> 18) & 0x3F];
        output[out_len++] = b64_table[(triple >> 12) & 0x3F];
        output[out_len++] = (i + 1 < input_len) ? b64_table[(triple >> 6) & 0x3F] : '=';
        output[out_len++] = '=';
    }
    if (out_len < output_max) output[out_len] = '\0';
    return out_len;
}

// ==== Command Parser (typical application pattern) ====

struct Command {
    char verb[16];
    char args[4][64];
    int arg_count;
};

int parse_command(const char *line, Command *cmd) {
    memset(cmd, 0, sizeof(Command));
    
    // Skip whitespace
    while (*line == ' ' || *line == '\t') line++;
    if (*line == '\0' || *line == '#') return -1;
    
    // Read verb
    int vi = 0;
    while (*line && *line != ' ' && *line != '\t' && vi < 15) {
        cmd->verb[vi++] = *line++;
    }
    cmd->verb[vi] = '\0';
    
    // Read arguments
    cmd->arg_count = 0;
    while (cmd->arg_count < 4) {
        while (*line == ' ' || *line == '\t') line++;
        if (*line == '\0' || *line == '#') break;
        
        int ai = 0;
        if (*line == '"') {
            // Quoted argument
            line++;
            while (*line && *line != '"' && ai < 63) {
                cmd->args[cmd->arg_count][ai++] = *line++;
            }
            if (*line == '"') line++;
        } else {
            while (*line && *line != ' ' && *line != '\t' && ai < 63) {
                cmd->args[cmd->arg_count][ai++] = *line++;
            }
        }
        cmd->args[cmd->arg_count][ai] = '\0';
        cmd->arg_count++;
    }
    
    return 0;
}

int execute_command(const Command *cmd) {
    if (strcmp(cmd->verb, "print") == 0 && cmd->arg_count >= 1) {
        printf("%s\n", cmd->args[0]);
        return 0;
    }
    if (strcmp(cmd->verb, "add") == 0 && cmd->arg_count >= 2) {
        int a = atoi(cmd->args[0]);
        int b = atoi(cmd->args[1]);
        printf("%d\n", a + b);
        return 0;
    }
    if (strcmp(cmd->verb, "echo") == 0) {
        for (int i = 0; i < cmd->arg_count; i++) {
            if (i > 0) printf(" ");
            printf("%s", cmd->args[i]);
        }
        printf("\n");
        return 0;
    }
    return -1;
}

// ==== Matrix operations (nested access patterns) ====

void matrix_transpose(const double *src, double *dst, int rows, int cols) {
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < cols; j++) {
            dst[j * rows + i] = src[i * cols + j];
        }
    }
}

double matrix_trace(const double *m, int n) {
    double sum = 0;
    for (int i = 0; i < n; i++) {
        sum += m[i * n + i];
    }
    return sum;
}

int main(int argc, char **argv) {
    // Sorting
    int data[] = {42, 17, 93, 5, 28, 64, 11, 76, 33, 50};
    int n = sizeof(data) / sizeof(data[0]);
    insertion_sort(data, n);
    printf("insertion sorted: ");
    for (int i = 0; i < n; i++) printf("%d ", data[i]);
    printf("\n");
    
    int data2[] = {42, 17, 93, 5, 28, 64, 11, 76, 33, 50};
    quicksort(data2, 0, n - 1);
    printf("quick sorted: ");
    for (int i = 0; i < n; i++) printf("%d ", data2[i]);
    printf("\n");
    
    // Binary search
    printf("bsearch(28) = %d\n", binary_search(data, n, 28));
    printf("bsearch(99) = %d\n", binary_search(data, n, 99));
    
    // Hash table
    HashTable ht;
    ht_init(&ht);
    ht_put(&ht, "alpha", 1);
    ht_put(&ht, "beta", 2);
    ht_put(&ht, "gamma", 3);
    int val;
    if (ht_get(&ht, "beta", &val) == 0) printf("ht[beta] = %d\n", val);
    
    // Ring buffer
    RingBuffer *rb = rb_create(8);
    for (int i = 0; i < 5; i++) rb_push(rb, 'A' + i);
    printf("ring: ");
    uint8_t b;
    while (rb_pop(rb, &b) == 0) printf("%c", b);
    printf("\n");
    rb_free(rb);
    
    // Base64
    char b64[128];
    base64_encode((const uint8_t *)"Hello!", 6, b64, 128);
    printf("base64(Hello!) = %s\n", b64);
    
    // Command parser
    Command cmd;
    parse_command("add 10 20", &cmd);
    execute_command(&cmd);
    parse_command("echo \"hello world\" test", &cmd);
    execute_command(&cmd);
    
    return 0;
}
