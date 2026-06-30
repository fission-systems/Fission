#include <stdio.h>
#include <stdint.h>

// Bit manipulation functions
uint32_t bit_reverse(uint32_t value) {
    uint32_t result = 0;
    for (int i = 0; i < 32; i++) {
        result = (result << 1) | (value & 1);
        value >>= 1;
    }
    return result;
}

// Count set bits
int popcount(uint32_t value) {
    int count = 0;
    while (value) {
        count += value & 1;
        value >>= 1;
    }
    return count;
}

// Find first set bit
int find_first_set_bit(uint32_t value) {
    if (value == 0) return -1;
    
    int position = 0;
    while ((value & 1) == 0) {
        value >>= 1;
        position++;
    }
    return position;
}

// XOR all values
uint32_t xor_all(uint32_t *values, int count) {
    uint32_t result = 0;
    for (int i = 0; i < count; i++) {
        result ^= values[i];
    }
    return result;
}

// Bit field manipulation
typedef struct {
    unsigned int flag1 : 1;
    unsigned int flag2 : 1;
    unsigned int flag3 : 1;
    unsigned int value : 29;
} BitField;

int process_bitfield(BitField *bf) {
    int result = 0;
    if (bf->flag1) result |= 0x01;
    if (bf->flag2) result |= 0x02;
    if (bf->flag3) result |= 0x04;
    result |= (bf->value & 0x1FFFFFFF) << 3;
    return result;
}

// Complex control flow with gotos
int validate_input(int x, int y) {
    if (x < 0) {
        goto error_x;
    }
    
    if (y < 0) {
        goto error_y;
    }
    
    if (x > 1000) {
        goto error_range;
    }
    
    if (y > 1000) {
        goto error_range;
    }
    
    return x + y;
    
error_x:
    return -1;
error_y:
    return -2;
error_range:
    return -3;
}

// Nested switch statements
int complex_switch(int a, int b) {
    switch (a) {
        case 1:
            switch (b) {
                case 10: return 100;
                case 20: return 200;
                default: return 0;
            }
            break;
        case 2:
            switch (b) {
                case 10: return 1000;
                case 20: return 2000;
                default: return 0;
            }
            break;
        default:
            return -1;
    }
}

// Complex loop with breaks and continues
int loop_complex(int *arr, int len) {
    int result = 0;
    
    for (int i = 0; i < len; i++) {
        if (arr[i] < 0) {
            continue;
        }
        
        if (arr[i] > 1000) {
            break;
        }
        
        for (int j = 0; j < arr[i]; j++) {
            result++;
            if (result > 10000) {
                break;
            }
        }
    }
    
    return result;
}

// State machine
typedef enum {
    STATE_IDLE,
    STATE_ACTIVE,
    STATE_PROCESSING,
    STATE_ERROR
} State;

int state_machine(State current, int input) {
    switch (current) {
        case STATE_IDLE:
            if (input == 1) return STATE_ACTIVE;
            break;
            
        case STATE_ACTIVE:
            if (input == 0) return STATE_IDLE;
            if (input == 2) return STATE_PROCESSING;
            break;
            
        case STATE_PROCESSING:
            if (input == 3) return STATE_ERROR;
            if (input == 0) return STATE_IDLE;
            break;
            
        case STATE_ERROR:
            if (input == 1) return STATE_IDLE;
            break;
    }
    
    return current;
}

// Main function
int main() {
    // Bit operations
    uint32_t test_val = 0xDEADBEEF;
    int reversed_bits = bit_reverse(test_val);
    int set_bits = popcount(test_val);
    int first_set = find_first_set_bit(test_val);
    
    // XOR operations
    uint32_t vals[] = {0x12345678, 0x87654321, 0xFFFFFFFF};
    uint32_t xor_result = xor_all(vals, 3);
    
    // Bitfield operations
    BitField bf = {1, 0, 1, 0x1FFFFFFF};
    int bf_result = process_bitfield(&bf);
    
    // Complex control flow
    int validate_result = validate_input(500, 600);
    int switch_result = complex_switch(2, 20);
    
    int arr[] = {100, 200, 50, 75};
    int loop_result = loop_complex(arr, 4);
    
    // State machine
    State state = STATE_IDLE;
    state = state_machine(state, 1);
    state = state_machine(state, 2);
    state = state_machine(state, 3);
    
    printf("Results: %d %d %d %x %d %d %d\n", 
           reversed_bits, set_bits, first_set, xor_result, 
           bf_result, validate_result, switch_result);
    
    return 0;
}
