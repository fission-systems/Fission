#include <iostream>
#include <vector>
#include <algorithm>

// Simple class for basic arithmetic
class Calculator {
private:
    int result;
    
public:
    Calculator() : result(0) {}
    
    int add(int a, int b) {
        result = a + b;
        return result;
    }
    
    int multiply(int a, int b) {
        result = a * b;
        return result;
    }
    
    int getResult() const {
        return result;
    }
};

// Template function for finding maximum
template <typename T>
T findMax(T a, T b) {
    return (a > b) ? a : b;
}

// Fibonacci with memoization using vector
class FibonacciCalc {
private:
    std::vector<int> cache;
    
public:
    FibonacciCalc(int maxN) : cache(maxN + 1, -1) {}
    
    int fibonacci(int n) {
        if (n <= 1) return n;
        if (cache[n] != -1) return cache[n];
        
        cache[n] = fibonacci(n - 1) + fibonacci(n - 2);
        return cache[n];
    }
};

// Function with virtual method
class Operation {
public:
    virtual ~Operation() = default;
    virtual int execute(int a, int b) = 0;
};

class AddOperation : public Operation {
public:
    int execute(int a, int b) override {
        return a + b;
    }
};

class MultiplyOperation : public Operation {
public:
    int execute(int a, int b) override {
        return a * b;
    }
};

// Array operations with vector
class ArrayProcessor {
private:
    std::vector<int> data;
    
public:
    ArrayProcessor(const std::vector<int>& arr) : data(arr) {}
    
    int sum() const {
        int total = 0;
        for (int val : data) {
            total += val;
        }
        return total;
    }
    
    int findMax() const {
        return *std::max_element(data.begin(), data.end());
    }
    
    void sort() {
        std::sort(data.begin(), data.end());
    }
};

// Template class for generic operations
template <typename T>
class Pair {
private:
    T first, second;
    
public:
    Pair(T f, T s) : first(f), second(s) {}
    
    T getFirst() const { return first; }
    T getSecond() const { return second; }
    
    T sum() const { return first + second; }
};

int main() {
    // Test Calculator class
    Calculator calc;
    int result1 = calc.add(5, 10);
    int result2 = calc.multiply(3, 7);
    
    // Test template function
    int max_int = findMax<int>(42, 58);
    double max_double = findMax<double>(3.14, 2.71);
    
    // Test Fibonacci with memoization
    FibonacciCalc fib(20);
    int fib_result = fib.fibonacci(10);
    
    // Test virtual functions
    AddOperation add_op;
    MultiplyOperation mul_op;
    int virt_result1 = add_op.execute(10, 20);
    int virt_result2 = mul_op.execute(4, 5);
    
    // Test vector operations
    std::vector<int> arr = {5, 2, 8, 1, 9, 3};
    ArrayProcessor processor(arr);
    int arr_sum = processor.sum();
    int arr_max = processor.findMax();
    processor.sort();
    
    // Test template class
    Pair<int> pair_int(10, 20);
    int pair_sum = pair_int.sum();
    
    Pair<double> pair_double(1.5, 2.5);
    double pair_double_sum = pair_double.sum();
    
    std::cout << "Results: " << result1 << " " << result2 << " " 
              << max_int << " " << fib_result << " " << arr_sum << std::endl;
    
    return 0;
}
