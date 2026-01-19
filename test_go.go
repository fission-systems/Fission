package main

import "fmt"

func main() {
	fmt.Println("Hello, Fission Go Analysis!")
	res := calculateSomething(10, 20)
	fmt.Printf("Result: %d\n", res)
}

//go:noinline
func calculateSomething(a, b int) int {
	return a*a + b*b
}
