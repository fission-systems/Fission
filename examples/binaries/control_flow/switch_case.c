/**
 * Test: Complex Control Flow - Switch-Case
 * Category: Control Flow
 * Difficulty: Medium
 */

#include <stdio.h>

#if defined(_MSC_VER)
#define NOINLINE __declspec(noinline)
#else
#define NOINLINE __attribute__((noinline))
#endif

// Test 1: Large switch with fall-through
const char* get_day_type(int day) {
    switch (day) {
        case 1:
        case 2:
        case 3:
        case 4:
        case 5:
            return "Weekday";
        case 6:
        case 7:
            return "Weekend";
        default:
            return "Invalid";
    }
}

// Test 2: Nested switch
int calculate_score(char grade, int attendance) {
    int base_score = 0;
    
    switch (grade) {
        case 'A':
        case 'a':
            base_score = 90;
            switch (attendance) {
                case 100: return base_score + 10;
                case 90:  return base_score + 5;
                default:  return base_score;
            }
        case 'B':
        case 'b':
            base_score = 80;
            break;
        case 'C':
        case 'c':
            base_score = 70;
            break;
        case 'D':
        case 'd':
            base_score = 60;
            break;
        case 'F':
        case 'f':
            return 0;
        default:
            return -1;
    }
    
    return base_score + (attendance / 10);
}

// Test 3: Switch with complex expressions
void process_command(int cmd, int arg) {
    switch (cmd) {
        case 0x01:
            printf("Initialize with %d\n", arg);
            break;
        case 0x02:
            printf("Process %d items\n", arg);
            break;
        case 0x10:
        case 0x11:
        case 0x12:
            printf("Network command 0x%02x: %d\n", cmd, arg);
            break;
        case 0x20:
            printf("Shutdown\n");
            break;
        default:
            printf("Unknown command: 0x%02x\n", cmd);
    }
}

// Test 4: Switch with string-like behavior (using first char)
int parse_simple_command(const char* str) {
    if (!str || str[0] == '\0') return -1;
    
    switch (str[0]) {
        case 'h':
        case 'H':
            if (str[1] == 'e' && str[2] == 'l' && str[3] == 'p') {
                printf("Help command\n");
                return 0;
            }
            break;
        case 'q':
        case 'Q':
            printf("Quit command\n");
            return 1;
        case 'r':
        case 'R':
            printf("Run command\n");
            return 2;
        default:
            printf("Unknown: %s\n", str);
            return -1;
    }
    return -1;
}

// Test 5: Dense switch for jump-table recovery
NOINLINE int dense_case0(int x) { return (x * 3) ^ 0x11; }
NOINLINE int dense_case1(int x) { return (x + 7) * 5; }
NOINLINE int dense_case2(int x) { return (x << 2) + 9; }
NOINLINE int dense_case3(int x) { return (x ^ 0x5a) + 1; }
NOINLINE int dense_case4(int x) { return (x * x) - 3; }
NOINLINE int dense_case5(int x) { return (x + 13) ^ 0x33; }
NOINLINE int dense_case6(int x) { return (x * 11) - 4; }
NOINLINE int dense_case7(int x) { return (x << 1) + 21; }
NOINLINE int dense_case8(int x) { return (x + 29) ^ 0x7; }
NOINLINE int dense_case9(int x) { return (x * 9) + 2; }
NOINLINE int dense_case10(int x) { return (x - 5) ^ 0x2d; }
NOINLINE int dense_case11(int x) { return (x << 3) - 6; }
NOINLINE int dense_case12(int x) { return (x + 1) * 17; }
NOINLINE int dense_case13(int x) { return (x ^ 0x3c) - 8; }
NOINLINE int dense_case14(int x) { return (x * 13) + 19; }
NOINLINE int dense_case15(int x) { return (x + 31) ^ 0x55; }

NOINLINE int dense_switch_table(int code) {
    switch (code) {
        case 0: return dense_case0(code);
        case 1: return dense_case1(code);
        case 2: return dense_case2(code);
        case 3: return dense_case3(code);
        case 4: return dense_case4(code);
        case 5: return dense_case5(code);
        case 6: return dense_case6(code);
        case 7: return dense_case7(code);
        case 8: return dense_case8(code);
        case 9: return dense_case9(code);
        case 10: return dense_case10(code);
        case 11: return dense_case11(code);
        case 12: return dense_case12(code);
        case 13: return dense_case13(code);
        case 14: return dense_case14(code);
        case 15: return dense_case15(code);
        default: return -1;
    }
}

int main(int argc, char** argv) {
    printf("=== Switch-Case Test ===\n\n");
    (void)argv;
    
    // Test 1
    printf("Day 3: %s\n", get_day_type(3));
    printf("Day 6: %s\n", get_day_type(6));
    
    // Test 2
    printf("Grade A, 100%% attendance: %d\n", calculate_score('A', 100));
    printf("Grade B, 85%% attendance: %d\n", calculate_score('B', 85));
    
    // Test 3
    process_command(0x01, 42);
    process_command(0x11, 100);
    
    // Test 4
    parse_simple_command("help");
    parse_simple_command("quit");

    // Test 5
    {
        volatile int selector = argc;
        int result = dense_switch_table(selector & 0x3f);
        printf("Dense switch: %d\n", result);
    }
    
    return 0;
}
