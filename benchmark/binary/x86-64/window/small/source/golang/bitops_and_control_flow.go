package main

import (
	"fmt"
)

// Bit reversal
func bitReverse(value uint32) uint32 {
	result := uint32(0)
	for i := 0; i < 32; i++ {
		result = (result << 1) | (value & 1)
		value >>= 1
	}
	return result
}

// Population count
func popcount(value uint32) int {
	count := 0
	for value > 0 {
		count += int(value & 1)
		value >>= 1
	}
	return count
}

// Find first set bit
func findFirstSetBit(value uint32) int {
	if value == 0 {
		return -1
	}
	pos := 0
	for (value & 1) == 0 {
		value >>= 1
		pos++
	}
	return pos
}

// XOR all values
func xorAll(values []uint32) uint32 {
	result := uint32(0)
	for _, v := range values {
		result ^= v
	}
	return result
}

// State type for state machine
type State int

const (
	IDLE State = iota
	ACTIVE
	PROCESSING
	ERROR
)

// StateMachine
type StateMachine struct {
	Current State
}

// Transition
func (sm *StateMachine) Transition(input int) State {
	switch sm.Current {
	case IDLE:
		if input == 1 {
			sm.Current = ACTIVE
		}
	case ACTIVE:
		if input == 0 {
			sm.Current = IDLE
		} else if input == 2 {
			sm.Current = PROCESSING
		}
	case PROCESSING:
		if input == 3 {
			sm.Current = ERROR
		} else if input == 0 {
			sm.Current = IDLE
		}
	case ERROR:
		if input == 1 {
			sm.Current = IDLE
		}
	}
	return sm.Current
}

// Validate input with early return
func validateInput(x, y int) int {
	if x < 0 {
		return -1
	}
	if y < 0 {
		return -2
	}
	if x > 1000 || y > 1000 {
		return -3
	}
	return x + y
}

// Complex switch
func complexSwitch(a, b int) int {
	switch a {
	case 1:
		switch b {
		case 10:
			return 100
		case 20:
			return 200
		default:
			return 0
		}
	case 2:
		switch b {
		case 10:
			return 1000
		case 20:
			return 2000
		default:
			return 0
		}
	default:
		return -1
	}
}

// Complex loop with breaks and continues
func complexLoop(arr []int) int {
	result := 0

	for _, val := range arr {
		if val < 0 {
			continue
		}
		if val > 1000 {
			break
		}

		for j := 0; j < val; j++ {
			result++
			if result > 10000 {
				break
			}
		}
	}

	return result
}

// Process matrix with nested loops
func processMatrix(matrix [][]int) int {
	sum := 0

	for i := range matrix {
		for j := range matrix[i] {
			if matrix[i][j] > 0 {
				sum += matrix[i][j]
			}
		}
	}

	return sum
}

func main() {
	// Test bit operations
	bfValue := uint32(0xDEADBEEF)
	reversed := bitReverse(bfValue)
	popcount_result := popcount(bfValue)
	firstSet := findFirstSetBit(bfValue)

	// Test state machine
	fsm := &StateMachine{Current: IDLE}
	s1 := fsm.Transition(1)
	s2 := fsm.Transition(2)
	s3 := fsm.Transition(3)
	fsm.Transition(0) // transition to another state

	// Test bit operations
	vals := []uint32{0x12345678, 0x87654321, 0xFFFFFFFF}
	xorResult := xorAll(vals)

	// Test control flow
	validateResult := validateInput(500, 600)
	switchResult := complexSwitch(2, 20)
	arr := []int{100, 200, 50, 75}
	loopResult := complexLoop(arr)

	// Test matrix operations
	matrix := [][]int{
		{1, 2, 3},
		{4, 5, 6},
		{7, 8, 9},
	}
	matrixSum := processMatrix(matrix)

	fmt.Printf("Bitops and control flow test: %d %d %d\n", popcount_result, switchResult, matrixSum)
	fmt.Printf("Bit reverse: %x, XOR: %x, Validate: %d\n", reversed, xorResult, validateResult)
	fmt.Printf("State transitions: %d -> %d -> %d -> %d\n", IDLE, s1, s2, s3)
	fmt.Printf("Loop result: %d, First set bit: %d\n", loopResult, firstSet)
}
