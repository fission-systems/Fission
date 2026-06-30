#include <iostream>
#include <cmath>
#include <vector>
#include <numeric>
#include <algorithm>

// Define PI for portability (MinGW compatibility)
#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

// Number theory utilities
class NumberTheory {
public:
    static int gcd(int a, int b) {
        while (b != 0) {
            int temp = b;
            b = a % b;
            a = temp;
        }
        return a;
    }
    
    static int lcm(int a, int b) {
        return (a / gcd(a, b)) * b;
    }
    
    static bool isPrime(int n) {
        if (n <= 1) return false;
        if (n <= 3) return true;
        if (n % 2 == 0 || n % 3 == 0) return false;
        
        for (int i = 5; i * i <= n; i += 6) {
            if (n % i == 0 || n % (i + 2) == 0) {
                return false;
            }
        }
        return true;
    }
};

// Factorial calculator with template
template <typename T>
class FactorialCalc {
public:
    static T factorial(T n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
    
    static T factorialIter(T n) {
        T result = 1;
        for (T i = 2; i <= n; i++) {
            result *= i;
        }
        return result;
    }
};

// Power calculator
class PowerCalc {
public:
    static long power(int base, int exp) {
        long result = 1;
        for (int i = 0; i < exp; i++) {
            result *= base;
        }
        return result;
    }
    
    static long modPower(long base, long exp, long mod) {
        long result = 1;
        base %= mod;
        
        while (exp > 0) {
            if (exp % 2 == 1) {
                result = (result * base) % mod;
            }
            exp = exp >> 1;
            base = (base * base) % mod;
        }
        
        return result;
    }
};

// Digit operations
class DigitOps {
public:
    static int sumOfDigits(int n) {
        int sum = 0;
        n = (n < 0) ? -n : n;
        
        while (n > 0) {
            sum += n % 10;
            n /= 10;
        }
        
        return sum;
    }
    
    static int countDigits(int n) {
        if (n == 0) return 1;
        n = (n < 0) ? -n : n;
        
        int count = 0;
        while (n > 0) {
            count++;
            n /= 10;
        }
        return count;
    }
    
    static int reverseInteger(int n) {
        int reversed = 0;
        bool is_negative = n < 0;
        n = is_negative ? -n : n;
        
        while (n > 0) {
            reversed = reversed * 10 + (n % 10);
            n /= 10;
        }
        
        return is_negative ? -reversed : reversed;
    }
    
    static bool isPalindrome(int n) {
        if (n < 0) return false;
        return n == reverseInteger(n);
    }
};

// Geometry utilities
class Geometry {
public:
    static double circleArea(double radius) {
        return M_PI * radius * radius;
    }
    
    static double circleCircumference(double radius) {
        return 2.0 * M_PI * radius;
    }
    
    static double sphereVolume(double radius) {
        return (4.0 / 3.0) * M_PI * radius * radius * radius;
    }
};

// Statistics utilities
class Statistics {
public:
    static double calculateMean(const std::vector<int>& arr) {
        if (arr.empty()) return 0.0;
        int sum = std::accumulate(arr.begin(), arr.end(), 0);
        return static_cast<double>(sum) / arr.size();
    }
    
    static double calculateVariance(const std::vector<int>& arr) {
        if (arr.empty()) return 0.0;
        
        double mean = calculateMean(arr);
        double variance = 0.0;
        
        for (int val : arr) {
            double diff = val - mean;
            variance += diff * diff;
        }
        
        return variance / arr.size();
    }
};

// Matrix operations
template <typename T>
class Matrix2x2 {
private:
    T a, b, c, d;
    
public:
    Matrix2x2(T a = 0, T b = 0, T c = 0, T d = 0)
        : a(a), b(b), c(c), d(d) {}
    
    Matrix2x2 multiply(const Matrix2x2& m) const {
        return Matrix2x2(
            a * m.a + b * m.c,
            a * m.b + b * m.d,
            c * m.a + d * m.c,
            c * m.b + d * m.d
        );
    }
    
    T determinant() const {
        return a * d - b * c;
    }
};

// Fibonacci with memoization
class FibonacciMemo {
private:
    std::vector<int> cache;
    
public:
    FibonacciMemo(int max_n) : cache(max_n + 1, -1) {}
    
    int fibonacci(int n) {
        if (n <= 1) return n;
        if (cache[n] != -1) return cache[n];
        
        cache[n] = fibonacci(n - 1) + fibonacci(n - 2);
        return cache[n];
    }
};

int main() {
    // Test number theory
    int gcd_result = NumberTheory::gcd(48, 18);
    int lcm_result = NumberTheory::lcm(12, 18);
    bool is_prime = NumberTheory::isPrime(17);
    
    // Test factorial
    long fact = FactorialCalc<int>::factorialIter(5);
    
    // Test power
    long pow_result = PowerCalc::power(2, 10);
    long mod_pow_result = PowerCalc::modPower(2, 100, 1000000007);
    
    // Test digit operations
    int digit_sum = DigitOps::sumOfDigits(12345);
    int digit_count = DigitOps::countDigits(98765);
    int reversed = DigitOps::reverseInteger(12345);
    bool is_palin = DigitOps::isPalindrome(121);
    
    // Test geometry
    double circle_area = Geometry::circleArea(5.0);
    double circle_circum = Geometry::circleCircumference(5.0);
    double sphere_vol = Geometry::sphereVolume(3.0);
    
    // Test statistics
    std::vector<int> data = {10, 20, 30, 40, 50};
    double mean = Statistics::calculateMean(data);
    double variance = Statistics::calculateVariance(data);
    
    // Test matrix operations
    Matrix2x2<int> m1(1, 2, 3, 4);
    Matrix2x2<int> m2(5, 6, 7, 8);
    Matrix2x2<int> product = m1.multiply(m2);
    int det = m1.determinant();
    
    // Test Fibonacci with memoization
    FibonacciMemo fib(20);
    int fib_result = fib.fibonacci(15);
    
    std::cout << "Math operations test completed: " 
              << gcd_result << " " << lcm_result << " " 
              << fact << " " << digit_sum << " " << fib_result << std::endl;
    
    return 0;
}
