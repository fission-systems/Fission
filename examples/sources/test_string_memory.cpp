/**
 * test_string_memory.cpp
 *
 * Tests decompiler string/memory pattern recovery:
 *  - String literal references
 *  - String manipulation (strlen, strcpy, strcat, strcmp)
 *  - Buffer operations (memset, memcpy, memmove)
 *  - Stack buffer handling
 *  - Heap allocation patterns (malloc/free, new/delete)
 *  - Loop-based string operations (idiom targets)
 *  - Format string parsing
 */
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

// ---- String literal references ----
const char* get_greeting(int lang) {
    switch (lang) {
    case 0: return "Hello, World!";
    case 1: return "Bonjour le monde!";
    case 2: return "Hallo Welt!";
    case 3: return "Hola Mundo!";
    default: return "Unknown language";
    }
}

// ---- Manual strlen (loop idiom → strlen recognition) ----
int my_strlen(const char *s) {
    int len = 0;
    while (*s != '\0') {
        s++;
        len++;
    }
    return len;
}

// ---- Manual memset zero (loop idiom → memset recognition) ----
void zero_buffer(char *buf, int size) {
    for (int i = 0; i < size; i++) {
        buf[i] = 0;
    }
}

// ---- Manual memcpy ----
void my_memcpy(void *dst, const void *src, int size) {
    char *d = (char *)dst;
    const char *s = (const char *)src;
    for (int i = 0; i < size; i++) {
        d[i] = s[i];
    }
}

// ---- String builder pattern ----
struct StringBuilder {
    char *buffer;
    int capacity;
    int length;
};

void sb_init(StringBuilder *sb, int capacity) {
    sb->buffer = (char *)malloc(capacity);
    sb->capacity = capacity;
    sb->length = 0;
    if (sb->buffer) sb->buffer[0] = '\0';
}

void sb_append(StringBuilder *sb, const char *str) {
    int slen = strlen(str);
    if (sb->length + slen >= sb->capacity) {
        int new_cap = sb->capacity * 2;
        if (new_cap < sb->length + slen + 1) {
            new_cap = sb->length + slen + 1;
        }
        char *new_buf = (char *)realloc(sb->buffer, new_cap);
        if (!new_buf) return;
        sb->buffer = new_buf;
        sb->capacity = new_cap;
    }
    memcpy(sb->buffer + sb->length, str, slen + 1);
    sb->length += slen;
}

void sb_free(StringBuilder *sb) {
    free(sb->buffer);
    sb->buffer = nullptr;
    sb->length = 0;
    sb->capacity = 0;
}

// ---- Stack buffer operations ----
int format_address(char *out, int max_len, uint32_t ip_bytes) {
    return snprintf(out, max_len, "%d.%d.%d.%d",
        (ip_bytes >> 24) & 0xFF,
        (ip_bytes >> 16) & 0xFF,
        (ip_bytes >> 8) & 0xFF,
        ip_bytes & 0xFF);
}

// ---- String comparison patterns ----
int classify_command(const char *cmd) {
    if (strcmp(cmd, "help") == 0) return 1;
    if (strcmp(cmd, "exit") == 0) return 2;
    if (strcmp(cmd, "quit") == 0) return 2;
    if (strcmp(cmd, "list") == 0) return 3;
    if (strcmp(cmd, "show") == 0) return 4;
    if (strcmp(cmd, "version") == 0) return 5;
    return 0;
}

// ---- Tokenizer (strchr / strtok pattern) ----
int count_words(const char *str) {
    int count = 0;
    int in_word = 0;
    while (*str) {
        if (*str == ' ' || *str == '\t' || *str == '\n') {
            in_word = 0;
        } else if (!in_word) {
            in_word = 1;
            count++;
        }
        str++;
    }
    return count;
}

// ---- Heap allocation with error checking ----
char* duplicate_string(const char *s) {
    int len = strlen(s);
    char *copy = (char *)malloc(len + 1);
    if (copy == nullptr) return nullptr;
    memcpy(copy, s, len + 1);
    return copy;
}

// ---- Array of strings ----
int find_string(const char **haystack, int count, const char *needle) {
    for (int i = 0; i < count; i++) {
        if (strcmp(haystack[i], needle) == 0) {
            return i;
        }
    }
    return -1;
}

// ---- Popcount (loop idiom → __builtin_popcount) ----
int popcount32(uint32_t v) {
    int count = 0;
    while (v != 0) {
        count++;
        v = v & (v - 1);
    }
    return count;
}

// ---- Buffer with struct overlay ----
struct PacketHeader {
    uint16_t type;
    uint16_t length;
    uint32_t sequence;
};

int parse_packet(const uint8_t *data, int data_len, PacketHeader *out_hdr) {
    if (data_len < (int)sizeof(PacketHeader)) return -1;
    memcpy(out_hdr, data, sizeof(PacketHeader));
    if (out_hdr->length > data_len) return -2;
    return 0;
}

int main(int argc, char **argv) {
    printf("greeting: %s\n", get_greeting(0));
    
    const char *test = "Hello, Fission!";
    printf("my_strlen = %d (expected %d)\n", my_strlen(test), (int)strlen(test));
    
    char buf[64];
    zero_buffer(buf, 64);
    format_address(buf, 64, 0xC0A80001);  // 192.168.0.1
    printf("address: %s\n", buf);
    
    printf("classify(help) = %d\n", classify_command("help"));
    printf("classify(exit) = %d\n", classify_command("exit"));
    printf("classify(foo) = %d\n", classify_command("foo"));
    
    printf("count_words = %d\n", count_words("one two three four"));
    
    char *dup = duplicate_string("cloned string");
    printf("dup = %s\n", dup);
    free(dup);
    
    StringBuilder sb;
    sb_init(&sb, 16);
    sb_append(&sb, "Hello");
    sb_append(&sb, ", ");
    sb_append(&sb, "World!");
    printf("sb = %s (len=%d, cap=%d)\n", sb.buffer, sb.length, sb.capacity);
    sb_free(&sb);
    
    printf("popcount(0xFF) = %d\n", popcount32(0xFF));
    printf("popcount(0xAAAA) = %d\n", popcount32(0xAAAA));
    
    const char *words[] = {"apple", "banana", "cherry", "date"};
    printf("find(cherry) = %d\n", find_string(words, 4, "cherry"));
    printf("find(grape) = %d\n", find_string(words, 4, "grape"));
    
    return 0;
}
