package main

import (
	"fmt"
	"sort"
	"strings"
)

// Comparison function type
type CompareFunc func(int, int) int

// Compare functions
func compareAscending(a, b int) int {
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

func compareDescending(a, b int) int {
	if a > b {
		return -1
	}
	if a < b {
		return 1
	}
	return 0
}

func compareAbsValue(a, b int) int {
	absA := a
	if a < 0 {
		absA = -a
	}
	absB := b
	if b < 0 {
		absB = -b
	}
	if absA < absB {
		return -1
	}
	if absA > absB {
		return 1
	}
	return 0
}

// Transform function type
type TransformFunc func(int) int

// Transform functions
func square(x int) int {
	return x * x
}

func cube(x int) int {
	return x * x * x
}

func negate(x int) int {
	return -x
}

// StringUtils
type StringUtils struct{}

// String operations
func (su *StringUtils) Reverse(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

func (su *StringUtils) ToUpper(s string) string {
	return strings.ToUpper(s)
}

func (su *StringUtils) ToLower(s string) string {
	return strings.ToLower(s)
}

func (su *StringUtils) Compare(s1, s2 string) int {
	return strings.Compare(s1, s2)
}

func (su *StringUtils) Concatenate(s1, s2 string) string {
	return s1 + s2
}

func (su *StringUtils) FindChar(s string, ch byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == ch {
			return i
		}
	}
	return -1
}

// ArrayProcessor with function pointers
type ArrayProcessor struct {
	Data []int
}

// ApplyTransform
func (ap *ArrayProcessor) ApplyTransform(fn TransformFunc) {
	for i := range ap.Data {
		ap.Data[i] = fn(ap.Data[i])
	}
}

// CustomSort
func (ap *ArrayProcessor) CustomSort(cmp CompareFunc) {
	sort.Slice(ap.Data, func(i, j int) bool {
		return cmp(ap.Data[i], ap.Data[j]) < 0
	})
}

// Lambda-like operations with functions
func filterEven(arr []int) []int {
	var result []int
	for _, v := range arr {
		if v%2 == 0 {
			result = append(result, v)
		}
	}
	return result
}

func mapSquare(arr []int) []int {
	result := make([]int, len(arr))
	for i, v := range arr {
		result[i] = v * v
	}
	return result
}

func fold(arr []int, initial int) int {
	result := initial
	for _, v := range arr {
		result += v
	}
	return result
}

// ForEach callback
func forEach(arr []int, callback func(int)) {
	for _, v := range arr {
		callback(v)
	}
}

func main() {
	// Test function pointers with sorting
	arr1 := []int{3, 1, 4, 1, 5, 9, 2, 6}
	arr2 := make([]int, len(arr1))
	copy(arr2, arr1)

	processor1 := &ArrayProcessor{Data: arr1}
	processor1.CustomSort(compareAscending)

	processor2 := &ArrayProcessor{Data: arr2}
	processor2.CustomSort(compareDescending)

	// Test transform
	squareArr := []int{1, 2, 3, 4, 5}
	processor3 := &ArrayProcessor{Data: squareArr}
	processor3.ApplyTransform(square)

	// Test string operations
	su := &StringUtils{}
	str1 := "Hello"
	str2 := "World"

	reversed := su.Reverse(str1)
	upper := su.ToUpper(str1)
	lower := su.ToLower(upper)
	cmpResult := su.Compare(str1, str2)
	concat := su.Concatenate(str1, str2)
	findResult := su.FindChar("Hello World", 'o')

	// Test functional operations
	data := []int{1, 2, 3, 4, 5, 6, 7, 8, 9}
	even := filterEven(data)
	squared := mapSquare(data)
	foldResult := fold(data, 0)

	// Test callbacks
	callbackSum := 0
	forEach(arr1, func(val int) {
		callbackSum += val
	})

	fmt.Printf("Function pointers and strings test: %d %d %d\n",
		len(reversed), findResult, foldResult)
	fmt.Printf("String ops: %s %s\n", upper, concat)
	fmt.Printf("Case: %s, Even count: %d, Squared count: %d\n", lower, len(even), len(squared))
	fmt.Printf("Callback sum: %d, Compare: %d\n", callbackSum, cmpResult)
}
