package main

import "fmt"

// Simple arithmetic functions
func add(a, b int) int {
	return a + b
}

func multiply(a, b int) int {
	return a * b
}

// Find maximum
func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// Fibonacci with memoization
func fibonacci(n int) int {
	cache := make(map[int]int)
	var fib func(int) int
	fib = func(x int) int {
		if x <= 1 {
			return x
		}
		if val, ok := cache[x]; ok {
			return val
		}
		result := fib(x-1) + fib(x-2)
		cache[x] = result
		return result
	}
	return fib(n)
}

// Sum array
func sumArray(arr []int) int {
	sum := 0
	for _, val := range arr {
		sum += val
	}
	return sum
}

// Process code with switch
func processCode(code int) int {
	switch code {
	case 1:
		return 10
	case 2:
		return 20
	case 3:
		return 30
	default:
		return 0
	}
}

// Fill matrix
func fillMatrix(rows, cols, value int) [][]int {
	matrix := make([][]int, rows)
	for i := range matrix {
		matrix[i] = make([]int, cols)
		for j := range matrix[i] {
			matrix[i][j] = value
		}
	}
	return matrix
}

// Swap two values
func swap(a, b *int) {
	*a, *b = *b, *a
}

func main() {
	x := 5
	y := 10

	result1 := add(x, y)
	result2 := max(x, y)
	result3 := fibonacci(10)

	arr := []int{1, 2, 3, 4, 5}
	arrSum := sumArray(arr)

	codeResult := processCode(2)

	swap(&x, &y)

	matrix := fillMatrix(3, 3, 5)

	fmt.Printf("Results: %d %d %d %d %d\n", result1, result2, result3, arrSum, codeResult)
	fmt.Printf("Matrix size: %d x %d\n", len(matrix), len(matrix[0]))
}
