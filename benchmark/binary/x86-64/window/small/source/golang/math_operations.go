package main

import (
	"fmt"
	"math"
)

// Number theory
func gcd(a, b int) int {
	for b != 0 {
		temp := b
		b = a % b
		a = temp
	}
	return a
}

func lcm(a, b int) int {
	return (a / gcd(a, b)) * b
}

func isPrime(n int) bool {
	if n <= 1 {
		return false
	}
	if n <= 3 {
		return true
	}
	if n%2 == 0 || n%3 == 0 {
		return false
	}

	for i := 5; i*i <= n; i += 6 {
		if n%i == 0 || n%(i+2) == 0 {
			return false
		}
	}
	return true
}

// Factorial
func factorial(n int) int64 {
	if n <= 1 {
		return 1
	}
	return int64(n) * factorial(n-1)
}

func factorialIter(n int) int64 {
	result := int64(1)
	for i := 2; i <= n; i++ {
		result *= int64(i)
	}
	return result
}

// Power
func power(base, exp int) int64 {
	result := int64(1)
	for i := 0; i < exp; i++ {
		result *= int64(base)
	}
	return result
}

// Modular exponentiation
func modPower(base, exp, mod int64) int64 {
	result := int64(1)
	base = base % mod

	for exp > 0 {
		if exp%2 == 1 {
			result = (result * base) % mod
		}
		exp = exp >> 1
		base = (base * base) % mod
	}

	return result
}

// Digit operations
func sumOfDigits(n int) int {
	sum := 0
	if n < 0 {
		n = -n
	}

	for n > 0 {
		sum += n % 10
		n /= 10
	}

	return sum
}

func countDigits(n int) int {
	if n == 0 {
		return 1
	}
	if n < 0 {
		n = -n
	}

	count := 0
	for n > 0 {
		count++
		n /= 10
	}
	return count
}

func reverseInteger(n int) int {
	reversed := 0
	isNegative := n < 0
	if isNegative {
		n = -n
	}

	for n > 0 {
		reversed = reversed*10 + (n % 10)
		n /= 10
	}

	if isNegative {
		return -reversed
	}
	return reversed
}

func isPalindrome(n int) bool {
	if n < 0 {
		return false
	}
	return n == reverseInteger(n)
}

// Geometry
func circleArea(radius float64) float64 {
	return math.Pi * radius * radius
}

func circleCircumference(radius float64) float64 {
	return 2.0 * math.Pi * radius
}

func sphereVolume(radius float64) float64 {
	return (4.0 / 3.0) * math.Pi * radius * radius * radius
}

// Statistics
func calculateMean(arr []int) float64 {
	if len(arr) == 0 {
		return 0.0
	}
	sum := 0
	for _, v := range arr {
		sum += v
	}
	return float64(sum) / float64(len(arr))
}

func calculateVariance(arr []int) float64 {
	if len(arr) == 0 {
		return 0.0
	}

	mean := calculateMean(arr)
	variance := 0.0

	for _, v := range arr {
		diff := float64(v) - mean
		variance += diff * diff
	}

	return variance / float64(len(arr))
}

// Matrix2x2
type Matrix2x2 struct {
	a, b, c, d float64
}

// Multiply matrices
func (m1 *Matrix2x2) Multiply(m2 *Matrix2x2) *Matrix2x2 {
	return &Matrix2x2{
		a: m1.a*m2.a + m1.b*m2.c,
		b: m1.a*m2.b + m1.b*m2.d,
		c: m1.c*m2.a + m1.d*m2.c,
		d: m1.c*m2.b + m1.d*m2.d,
	}
}

// Determinant
func (m *Matrix2x2) Determinant() float64 {
	return m.a*m.d - m.b*m.c
}

// Fibonacci with memoization
func fibonacci(n int, cache map[int]int) int {
	if n <= 1 {
		return n
	}
	if val, ok := cache[n]; ok {
		return val
	}

	result := fibonacci(n-1, cache) + fibonacci(n-2, cache)
	cache[n] = result
	return result
}

func main() {
	// Test number theory
	gcdResult := gcd(48, 18)
	lcmResult := lcm(12, 18)
	isPrimeResult := isPrime(17)

	// Test factorial
	factResult := factorialIter(5)

	// Test power
	powResult := power(2, 10)
	modPowResult := modPower(2, 100, 1000000007)

	// Test digit operations
	digitSum := sumOfDigits(12345)
	digitCount := countDigits(98765)
	reversed := reverseInteger(12345)
	isPalinResult := isPalindrome(121)

	// Test geometry
	circleAreaResult := circleArea(5.0)
	circlCircumResult := circleCircumference(5.0)
	sphereVolResult := sphereVolume(3.0)

	// Test statistics
	data := []int{10, 20, 30, 40, 50}
	mean := calculateMean(data)
	variance := calculateVariance(data)

	// Test matrix operations
	m1 := &Matrix2x2{a: 1, b: 2, c: 3, d: 4}
	m2 := &Matrix2x2{a: 5, b: 6, c: 7, d: 8}
	product := m1.Multiply(m2)
	det := m1.Determinant()

	// Test Fibonacci with memoization
	cache := make(map[int]int)
	fibResult := fibonacci(15, cache)

	fmt.Printf("Math operations test: %d %d %d %d\n",
		gcdResult, lcmResult, int(factResult), digitSum)
	fmt.Printf("Powers: %d %d, Fibonacci: %d\n", powResult, modPowResult, fibResult)
	fmt.Printf("Geometry: area=%.2f, circumference=%.2f, volume=%.2f\n",
		circleAreaResult, circlCircumResult, sphereVolResult)
	fmt.Printf("Statistics: mean=%.2f, variance=%.2f\n", mean, variance)
	fmt.Printf("Matrix determinant: %.2f, Product a=%.2f\n", det, product.a)
	fmt.Printf("Palindrome: %v, Digits: %d, Prime: %v, Reversed: %d\n", isPalinResult, digitCount, isPrimeResult, reversed)
}
