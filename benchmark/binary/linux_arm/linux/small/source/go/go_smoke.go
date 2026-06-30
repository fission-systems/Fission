package main

import (
	"fmt"
)

var GoSink uint32 = 0

type Computable interface {
	Compute() uint32
}

type Adder struct {
	Amount uint32
}

func (a Adder) Compute() uint32 {
	return a.Amount + 10
}

type Multiplier struct {
	Factor uint32
}

func (m Multiplier) Compute() uint32 {
	return m.Factor * 5
}

func process(c Computable) uint32 {
	return c.Compute()
}

//export go_smoke
func go_smoke(seed uint32) uint32 {
	var total uint32 = 0
	items := []Computable{
		Adder{Amount: seed},
		Multiplier{Factor: seed},
	}
	
	for _, item := range items {
		total += process(item)
	}
	
	GoSink = total
	return total
}

func main() {
	// A simple entry point that ensures our code is not optimized out.
	// The binary will be an executable for decompilation benchmarks.
	fmt.Printf("go_smoke output: %d\n", go_smoke(10))
}
