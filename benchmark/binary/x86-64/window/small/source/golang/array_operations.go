package main

import (
	"fmt"
	"sort"
)

// Array operations
func findMax(arr []int) int {
	if len(arr) == 0 {
		return 0
	}
	max := arr[0]
	for _, v := range arr {
		if v > max {
			max = v
		}
	}
	return max
}

func findMin(arr []int) int {
	if len(arr) == 0 {
		return 0
	}
	min := arr[0]
	for _, v := range arr {
		if v < min {
			min = v
		}
	}
	return min
}

func arraySum(arr []int) int {
	sum := 0
	for _, v := range arr {
		sum += v
	}
	return sum
}

func arrayAverage(arr []int) float64 {
	if len(arr) == 0 {
		return 0.0
	}
	return float64(arraySum(arr)) / float64(len(arr))
}

// Binary search
func binarySearch(arr []int, target int) int {
	left := 0
	right := len(arr) - 1

	for left <= right {
		mid := left + (right-left)/2

		if arr[mid] == target {
			return mid
		} else if arr[mid] < target {
			left = mid + 1
		} else {
			right = mid - 1
		}
	}

	return -1
}

// Linear search
func linearSearch(arr []int, target int) int {
	for i, v := range arr {
		if v == target {
			return i
		}
	}
	return -1
}

// Merge arrays
func mergeArrays(arr1, arr2 []int) []int {
	result := make([]int, 0, len(arr1)+len(arr2))

	i, j := 0, 0
	for i < len(arr1) && j < len(arr2) {
		if arr1[i] <= arr2[j] {
			result = append(result, arr1[i])
			i++
		} else {
			result = append(result, arr2[j])
			j++
		}
	}

	for i < len(arr1) {
		result = append(result, arr1[i])
		i++
	}

	for j < len(arr2) {
		result = append(result, arr2[j])
		j++
	}

	return result
}

// Rotate array
func rotateArray(arr []int, k int) {
	if len(arr) == 0 || k == 0 {
		return
	}

	k = k % len(arr)

	// Reverse entire array
	reverse(arr, 0, len(arr)-1)
	// Reverse first k elements
	reverse(arr, 0, k-1)
	// Reverse remaining elements
	reverse(arr, k, len(arr)-1)
}

func reverse(arr []int, start, end int) {
	for start < end {
		arr[start], arr[end] = arr[end], arr[start]
		start++
		end--
	}
}

// Remove duplicates
func removeDuplicates(arr []int) []int {
	if len(arr) <= 1 {
		return arr
	}

	writeIdx := 0
	for i := 1; i < len(arr); i++ {
		if arr[i] != arr[writeIdx] {
			writeIdx++
			arr[writeIdx] = arr[i]
		}
	}

	return arr[:writeIdx+1]
}

// Bubble sort
func bubbleSort(arr []int) {
	for i := 0; i < len(arr)-1; i++ {
		for j := 0; j < len(arr)-i-1; j++ {
			if arr[j] > arr[j+1] {
				arr[j], arr[j+1] = arr[j+1], arr[j]
			}
		}
	}
}

// Insertion sort
func insertionSort(arr []int) {
	for i := 1; i < len(arr); i++ {
		key := arr[i]
		j := i - 1

		for j >= 0 && arr[j] > key {
			arr[j+1] = arr[j]
			j--
		}
		arr[j+1] = key
	}
}

// 2D array operations
func matrixSum(matrix [][]int) int {
	total := 0
	for _, row := range matrix {
		for _, val := range row {
			total += val
		}
	}
	return total
}

func transposeMatrix(src [][]int) [][]int {
	if len(src) == 0 {
		return [][]int{}
	}

	rows := len(src)
	cols := len(src[0])

	result := make([][]int, cols)
	for i := range result {
		result[i] = make([]int, rows)
	}

	for i := 0; i < rows; i++ {
		for j := 0; j < cols; j++ {
			result[j][i] = src[i][j]
		}
	}

	return result
}

func main() {
	// Test basic array operations
	arr := []int{3, 7, 2, 9, 1, 5}
	maxVal := findMax(arr)
	minVal := findMin(arr)
	sum := arraySum(arr)
	avg := arrayAverage(arr)

	// Test sorting algorithms
	arrBubble := []int{5, 2, 8, 1, 9}
	arrInsertion := []int{5, 2, 8, 1, 9}
	arrQuick := []int{5, 2, 8, 1, 9}

	bubbleSort(arrBubble)
	insertionSort(arrInsertion)
	sort.Ints(arrQuick)

	// Test binary search
	sortedArr := []int{1, 2, 3, 4, 5, 6, 7, 8, 9}
	searchResult := binarySearch(sortedArr, 5)

	// Test merge arrays
	arr1 := []int{1, 3, 5}
	arr2 := []int{2, 4, 6}
	merged := mergeArrays(arr1, arr2)

	// Test rotate array
	arrRotate := []int{1, 2, 3, 4, 5}
	rotateArray(arrRotate, 2)

	// Test remove duplicates
	arrDup := []int{1, 1, 2, 2, 3, 3, 3}
	newArr := removeDuplicates(arrDup)

	// Test 2D array operations
	matrix := [][]int{
		{1, 2, 3},
		{4, 5, 6},
		{7, 8, 9},
	}
	matrixTotal := matrixSum(matrix)
	transposed := transposeMatrix(matrix)

	fmt.Printf("Array operations test: %d %d %d\n", maxVal, minVal, len(newArr))
	fmt.Printf("Average: %.2f, Sum: %d\n", avg, sum)
	fmt.Printf("Search result: %d, Merged size: %d\n", searchResult, len(merged))
	fmt.Printf("Matrix sum: %d, Transposed size: %dx%d\n", matrixTotal, len(transposed), len(transposed[0]))
}
