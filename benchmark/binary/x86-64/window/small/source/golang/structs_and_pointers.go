package main

import "fmt"

// Record struct
type Record struct {
	ID    int
	Value int
	Name  string
}

// Process record
func (r *Record) Process() int {
	return r.ID + r.Value
}

// Node for linked list
type Node struct {
	Data int
	Next *Node
	Prev *Node
}

// LinkedList implementation
type LinkedList struct {
	Head *Node
}

// Insert at head
func (ll *LinkedList) InsertAtHead(data int) {
	newNode := &Node{Data: data}
	if ll.Head != nil {
		newNode.Next = ll.Head
		ll.Head.Prev = newNode
	}
	ll.Head = newNode
}

// Sum all nodes
func (ll *LinkedList) Sum() int {
	sum := 0
	current := ll.Head
	for current != nil {
		sum += current.Data
		current = current.Next
	}
	return sum
}

// Get length
func (ll *LinkedList) Length() int {
	count := 0
	current := ll.Head
	for current != nil {
		count++
		current = current.Next
	}
	return count
}

// Point structure with generics simulation
type Point struct {
	X, Y, Z float64
}

// Distance from origin
func (p *Point) DistanceFromOrigin() float64 {
	return p.X*p.X + p.Y*p.Y + p.Z*p.Z
}

// Translate point
func (p *Point) Translate(dx, dy, dz float64) {
	p.X += dx
	p.Y += dy
	p.Z += dz
}

// RecordManager
type RecordManager struct {
	Records []Record
}

// Add record
func (rm *RecordManager) AddRecord(rec Record) {
	rm.Records = append(rm.Records, rec)
}

// Get total value
func (rm *RecordManager) GetTotalValue() int {
	total := 0
	for _, rec := range rm.Records {
		total += rec.Value
	}
	return total
}

// Count records
func (rm *RecordManager) Count() int {
	return len(rm.Records)
}

// Pointer arithmetic with slices
type VectorOps struct {
	Data []int
}

// Sum via slice iteration
func (vo *VectorOps) Sum() int {
	sum := 0
	for _, val := range vo.Data {
		sum += val
	}
	return sum
}

// Modify via reference
func (vo *VectorOps) ModifyAt(index, value int) {
	if index >= 0 && index < len(vo.Data) {
		vo.Data[index] = value
	}
}

func main() {
	// Test Record
	rec1 := &Record{ID: 1, Value: 100, Name: "Alice"}
	rec2 := &Record{ID: 2, Value: 200, Name: "Bob"}
	calc1 := rec1.Process()
	calc2 := rec2.Process()

	// Test LinkedList
	list := &LinkedList{}
	list.InsertAtHead(10)
	list.InsertAtHead(20)
	list.InsertAtHead(30)
	list.InsertAtHead(40)
	listSum := list.Sum()
	listLen := list.Length()

	// Test Point
	point1 := &Point{X: 1.0, Y: 2.0, Z: 3.0}
	dist := point1.DistanceFromOrigin()
	point1.Translate(1.0, 1.0, 1.0)

	// Test RecordManager
	manager := &RecordManager{}
	manager.AddRecord(*rec1)
	manager.AddRecord(*rec2)
	totalValue := manager.GetTotalValue()
	recCount := manager.Count()

	// Test VectorOps
	arr := []int{1, 2, 3, 4, 5}
	vecOps := &VectorOps{Data: arr}
	vecSum := vecOps.Sum()
	vecOps.ModifyAt(0, 10)

	fmt.Printf("Structs and pointers test: %d %d %d %d %d %d\n",
		calc1, calc2, listSum, totalValue, int(dist), vecSum)
	fmt.Printf("List length: %d, Record count: %d\n", listLen, recCount)
}
