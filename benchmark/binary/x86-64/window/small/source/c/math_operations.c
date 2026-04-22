#include <stdio.h>
#include <math.h>
#include <string.h>

// Basic math operations
int gcd(int a, int b) {
    while (b != 0) {
        int temp = b;
        b = a % b;
        a = temp;
    }
    return a;
}

int lcm(int a, int b) {
    return (a / gcd(a, b)) * b;
}

// Prime number check
int is_prime(int n) {
    if (n <= 1) return 0;
    if (n <= 3) return 1;
    if (n % 2 == 0 || n % 3 == 0) return 0;
    
    for (int i = 5; i * i <= n; i += 6) {
        if (n % i == 0 || n % (i + 2) == 0) {
            return 0;
        }
    }
    return 1;
}

// Factorial
long factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

// Iterative factorial
long factorial_iter(int n) {
    long result = 1;
    for (int i = 2; i <= n; i++) {
        result *= i;
    }
    return result;
}

// Power function
long power(int base, int exp) {
    long result = 1;
    for (int i = 0; i < exp; i++) {
        result *= base;
    }
    return result;
}

// Modular exponentiation
long mod_power(long base, long exp, long mod) {
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

// Sum of digits
int sum_of_digits(int n) {
    int sum = 0;
    n = n < 0 ? -n : n;
    
    while (n > 0) {
        sum += n % 10;
        n /= 10;
    }
    
    return sum;
}

// Count digits
int count_digits(int n) {
    if (n == 0) return 1;
    n = n < 0 ? -n : n;
    
    int count = 0;
    while (n > 0) {
        count++;
        n /= 10;
    }
    return count;
}

// Reverse integer
int reverse_integer(int n) {
    int reversed = 0;
    int is_negative = n < 0;
    n = is_negative ? -n : n;
    
    while (n > 0) {
        reversed = reversed * 10 + (n % 10);
        n /= 10;
    }
    
    return is_negative ? -reversed : reversed;
}

// Palindrome check
int is_palindrome_number(int n) {
    if (n < 0) return 0;
    return n == reverse_integer(n);
}

// Floating point operations
double circle_area(double radius) {
    return 3.14159 * radius * radius;
}

double circle_circumference(double radius) {
    return 2.0 * 3.14159 * radius;
}

double sphere_volume(double radius) {
    return (4.0 / 3.0) * 3.14159 * radius * radius * radius;
}

// Mean and variance
double calculate_mean(int *arr, int len) {
    int sum = 0;
    for (int i = 0; i < len; i++) {
        sum += arr[i];
    }
    return (double)sum / len;
}

double calculate_variance(int *arr, int len) {
    double mean = calculate_mean(arr, len);
    double variance = 0.0;
    
    for (int i = 0; i < len; i++) {
        double diff = arr[i] - mean;
        variance += diff * diff;
    }
    
    return variance / len;
}

// Matrix operations (2x2)
typedef struct {
    double a, b;
    double c, d;
} Matrix2x2;

Matrix2x2 matrix_multiply(Matrix2x2 m1, Matrix2x2 m2) {
    Matrix2x2 result;
    result.a = m1.a * m2.a + m1.b * m2.c;
    result.b = m1.a * m2.b + m1.b * m2.d;
    result.c = m1.c * m2.a + m1.d * m2.c;
    result.d = m1.c * m2.b + m1.d * m2.d;
    return result;
}

double matrix_determinant(Matrix2x2 m) {
    return m.a * m.d - m.b * m.c;
}

// Fibonacci with memoization
int fib_memo[100];

int fibonacci_memo(int n) {
    if (n <= 1) return n;
    if (fib_memo[n] != -1) return fib_memo[n];
    
    fib_memo[n] = fibonacci_memo(n - 1) + fibonacci_memo(n - 2);
    return fib_memo[n];
}

// Main function
int main() {
    // GCD and LCM
    int gcd_result = gcd(48, 18);
    int lcm_result = lcm(12, 18);
    
    // Prime check
    int prime_check = is_prime(17);
    
    // Factorial
    long fact = factorial_iter(5);
    
    // Power and modular exponentiation
    long pow_result = power(2, 10);
    long mod_pow_result = mod_power(2, 100, 1000000007);
    
    // Number operations
    int digit_sum = sum_of_digits(12345);
    int digit_count = count_digits(98765);
    int reversed = reverse_integer(12345);
    int is_palin = is_palindrome_number(121);
    
    // Circle operations
    double area = circle_area(5.0);
    double circumference = circle_circumference(5.0);
    double vol = sphere_volume(3.0);
    
    // Statistics
    int data[] = {10, 20, 30, 40, 50};
    double mean = calculate_mean(data, 5);
    double variance = calculate_variance(data, 5);
    
    // Matrix operations
    Matrix2x2 m1 = {1, 2, 3, 4};
    Matrix2x2 m2 = {5, 6, 7, 8};
    Matrix2x2 product = matrix_multiply(m1, m2);
    double det = matrix_determinant(m1);
    
    // Fibonacci memo
    memset(fib_memo, -1, sizeof(fib_memo));
    int fib_result = fibonacci_memo(20);
    
    printf("Math results: %d %d %ld %d\n", gcd_result, lcm_result, fact, fib_result);
    
    return 0;
}
