#include <iostream>
#include <bitset>
#include <cstdint>
#include <cmath>

// Bitfield using template
template <typename T>
class BitField {
private:
    T value;
    
public:
    BitField(T initial = 0) : value(initial) {}
    
    void setBit(int pos) {
        value |= (1 << pos);
    }
    
    void clearBit(int pos) {
        value &= ~(1 << pos);
    }
    
    bool getBit(int pos) const {
        return (value >> pos) & 1;
    }
    
    int countBits() const {
        int count = 0;
        T temp = value;
        while (temp) {
            count += temp & 1;
            temp >>= 1;
        }
        return count;
    }
    
    T getValue() const { return value; }
};

// Enum for state machine
enum class State {
    IDLE,
    ACTIVE,
    PROCESSING,
    ERROR
};

// State machine class
class StateMachine {
private:
    State current_state;
    
public:
    StateMachine() : current_state(State::IDLE) {}
    
    State transition(int input) {
        switch (current_state) {
            case State::IDLE:
                if (input == 1) current_state = State::ACTIVE;
                break;
                
            case State::ACTIVE:
                if (input == 0) current_state = State::IDLE;
                else if (input == 2) current_state = State::PROCESSING;
                break;
                
            case State::PROCESSING:
                if (input == 3) current_state = State::ERROR;
                else if (input == 0) current_state = State::IDLE;
                break;
                
            case State::ERROR:
                if (input == 1) current_state = State::IDLE;
                break;
        }
        return current_state;
    }
    
    State getState() const { return current_state; }
};

// Bit manipulation utilities
class BitOps {
public:
    static uint32_t bitReverse(uint32_t value) {
        uint32_t result = 0;
        for (int i = 0; i < 32; i++) {
            result = (result << 1) | (value & 1);
            value >>= 1;
        }
        return result;
    }
    
    static int popcount(uint32_t value) {
        int count = 0;
        while (value) {
            count += value & 1;
            value >>= 1;
        }
        return count;
    }
    
    static int findFirstSetBit(uint32_t value) {
        if (value == 0) return -1;
        int pos = 0;
        while ((value & 1) == 0) {
            value >>= 1;
            pos++;
        }
        return pos;
    }
    
    static uint32_t xorAll(const uint32_t* values, int count) {
        uint32_t result = 0;
        for (int i = 0; i < count; i++) {
            result ^= values[i];
        }
        return result;
    }
};

// Complex control flow handler
class ControlFlowHandler {
public:
    static int validateInput(int x, int y) {
        if (x < 0) return -1;
        if (y < 0) return -2;
        if (x > 1000) return -3;
        if (y > 1000) return -3;
        return x + y;
    }
    
    static int complexSwitch(int a, int b) {
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
    
    static int complexLoop(const int* arr, int len) {
        int result = 0;
        
        for (int i = 0; i < len; i++) {
            if (arr[i] < 0) continue;
            if (arr[i] > 1000) break;
            
            for (int j = 0; j < arr[i]; j++) {
                result++;
                if (result > 10000) break;
            }
        }
        
        return result;
    }
};

// Nested control structures
class NestedStructures {
public:
    static int processMatrix(int matrix[3][3]) {
        int sum = 0;
        
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (matrix[i][j] > 0) {
                    sum += matrix[i][j];
                }
            }
        }
        
        return sum;
    }
};

int main() {
    // Test BitField
    BitField<uint32_t> bf(0xDEADBEEF);
    bf.setBit(0);
    bf.clearBit(1);
    int bits_count = bf.countBits();
    uint32_t reversed = BitOps::bitReverse(0xDEADBEEF);
    int popcount_result = BitOps::popcount(0xDEADBEEF);
    
    // Test state machine
    StateMachine fsm;
    State s1 = fsm.transition(1);
    State s2 = fsm.transition(2);
    State s3 = fsm.transition(3);
    State s4 = fsm.getState();
    
    // Test bit operations
    uint32_t vals[] = {0x12345678, 0x87654321, 0xFFFFFFFF};
    uint32_t xor_result = BitOps::xorAll(vals, 3);
    int first_set = BitOps::findFirstSetBit(0xDEADBEEF);
    
    // Test control flow
    int validate_result = ControlFlowHandler::validateInput(500, 600);
    int switch_result = ControlFlowHandler::complexSwitch(2, 20);
    int arr[] = {100, 200, 50, 75};
    int loop_result = ControlFlowHandler::complexLoop(arr, 4);
    
    // Test nested structures
    int matrix[3][3] = {
        {1, 2, 3},
        {4, 5, 6},
        {7, 8, 9}
    };
    int matrix_sum = NestedStructures::processMatrix(matrix);
    
    std::cout << "Bitops and control flow test completed: " 
              << bits_count << " " << popcount_result << " " 
              << switch_result << " " << matrix_sum << std::endl;
    
    return 0;
}
