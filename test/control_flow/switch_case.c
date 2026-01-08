/**
 * Test: Complex Control Flow - Switch-Case
 * Category: Control Flow
 * Difficulty: Medium
 */

#include <stdio.h>

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

int main() {
    printf("=== Switch-Case Test ===\n\n");
    
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
    
    return 0;
}
