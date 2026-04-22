package main

import (
	"errors"
	"fmt"
	"sync"
)

// ============================================================================
// Graph Algorithms (BFS, DFS, Topological Sort)
// ============================================================================

type GraphNode struct {
	ID    int
	Edges []*GraphNode
}

type Graph struct {
	nodes map[int]*GraphNode
	mu    sync.RWMutex
}

func (g *Graph) AddNode(id int) {
	g.mu.Lock()
	defer g.mu.Unlock()
	if _, exists := g.nodes[id]; !exists {
		g.nodes[id] = &GraphNode{ID: id, Edges: make([]*GraphNode, 0)}
	}
}

func (g *Graph) AddEdge(from, to int) error {
	g.mu.RLock()
	defer g.mu.RUnlock()

	fromNode, exists := g.nodes[from]
	if !exists {
		return errors.New("source node not found")
	}

	toNode, exists := g.nodes[to]
	if !exists {
		return errors.New("destination node not found")
	}

	for _, edge := range fromNode.Edges {
		if edge.ID == to {
			return nil // Edge already exists
		}
	}

	fromNode.Edges = append(fromNode.Edges, toNode)
	return nil
}

func (g *Graph) BFS(startID int) []int {
	g.mu.RLock()
	startNode, exists := g.nodes[startID]
	g.mu.RUnlock()

	if !exists {
		return []int{}
	}

	visited := make(map[int]bool)
	queue := make([]*GraphNode, 0)
	result := make([]int, 0)

	queue = append(queue, startNode)
	visited[startID] = true

	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		result = append(result, current.ID)

		for _, neighbor := range current.Edges {
			if !visited[neighbor.ID] {
				visited[neighbor.ID] = true
				queue = append(queue, neighbor)
			}
		}
	}

	return result
}

func (g *Graph) DFS(startID int, visited map[int]bool, result *[]int) error {
	g.mu.RLock()
	startNode, exists := g.nodes[startID]
	g.mu.RUnlock()

	if !exists {
		return errors.New("node not found")
	}

	visited[startID] = true
	*result = append(*result, startID)

	for _, neighbor := range startNode.Edges {
		if !visited[neighbor.ID] {
			g.DFS(neighbor.ID, visited, result)
		}
	}

	return nil
}

// ============================================================================
// Concurrent Processing with Enhanced Worker Pool
// ============================================================================

type Task struct {
	ID    int
	Value int
	Error error
}

type WorkerPool struct {
	workers  int
	jobs     chan *Task
	results  chan *Task
	wg       sync.WaitGroup
	mu       sync.Mutex
	jobCount int
}

func NewWorkerPool(numWorkers int) *WorkerPool {
	return &WorkerPool{
		workers: numWorkers,
		jobs:    make(chan *Task, numWorkers*2),
		results: make(chan *Task, numWorkers*2),
	}
}

func (wp *WorkerPool) Start() {
	for i := 0; i < wp.workers; i++ {
		wp.wg.Add(1)
		go wp.worker(i)
	}
}

func (wp *WorkerPool) worker(id int) {
	defer wp.wg.Done()
	for task := range wp.jobs {
		task.Value = task.Value*2 + id
		wp.results <- task
	}
}

func (wp *WorkerPool) Submit(task *Task) {
	wp.mu.Lock()
	wp.jobCount++
	wp.mu.Unlock()
	wp.jobs <- task
}

func (wp *WorkerPool) Stop() {
	close(wp.jobs)
	wp.wg.Wait()
	close(wp.results)
}

// ============================================================================
// Generic Container Interface
// ============================================================================

type Container interface {
	Push(value interface{})
	Pop() (interface{}, error)
	IsEmpty() bool
	Size() int
}

type Stack struct {
	items []interface{}
	mu    sync.Mutex
}

func (s *Stack) Push(value interface{}) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.items = append(s.items, value)
}

func (s *Stack) Pop() (interface{}, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.items) == 0 {
		return nil, errors.New("stack is empty")
	}
	item := s.items[len(s.items)-1]
	s.items = s.items[:len(s.items)-1]
	return item, nil
}

func (s *Stack) IsEmpty() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.items) == 0
}

func (s *Stack) Size() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.items)
}

// ============================================================================
// Advanced Sorting Algorithms
// ============================================================================

type Sorter struct {
	data []int
}

func (s *Sorter) HeapSort() {
	n := len(s.data)
	for i := n/2 - 1; i >= 0; i-- {
		s.heapify(n, i)
	}
	for i := n - 1; i > 0; i-- {
		s.data[0], s.data[i] = s.data[i], s.data[0]
		s.heapify(i, 0)
	}
}

func (s *Sorter) heapify(n, i int) {
	largest := i
	left := 2*i + 1
	right := 2*i + 2

	if left < n && s.data[left] > s.data[largest] {
		largest = left
	}
	if right < n && s.data[right] > s.data[largest] {
		largest = right
	}

	if largest != i {
		s.data[i], s.data[largest] = s.data[largest], s.data[i]
		s.heapify(n, largest)
	}
}

func (s *Sorter) QuickSort() {
	if len(s.data) <= 1 {
		return
	}
	s.quickSort(0, len(s.data)-1)
}

func (s *Sorter) quickSort(low, high int) {
	if low < high {
		pi := s.partition(low, high)
		s.quickSort(low, pi-1)
		s.quickSort(pi+1, high)
	}
}

func (s *Sorter) partition(low, high int) int {
	pivot := s.data[high]
	i := low - 1

	for j := low; j < high; j++ {
		if s.data[j] < pivot {
			i++
			s.data[i], s.data[j] = s.data[j], s.data[i]
		}
	}
	s.data[i+1], s.data[high] = s.data[high], s.data[i+1]
	return i + 1
}

// ============================================================================
// String Processing with Advanced Features
// ============================================================================

type StringProcessor struct{}

func (sp *StringProcessor) FindAllOccurrences(text, pattern string) []int {
	var positions []int
	for i := 0; i <= len(text)-len(pattern); i++ {
		if text[i:i+len(pattern)] == pattern {
			positions = append(positions, i)
		}
	}
	return positions
}

func (sp *StringProcessor) LongestCommonSubstring(s1, s2 string) string {
	if len(s1) == 0 || len(s2) == 0 {
		return ""
	}

	m, n := len(s1), len(s2)
	matrix := make([][]int, m+1)
	for i := range matrix {
		matrix[i] = make([]int, n+1)
	}

	maxLen := 0
	endPos := 0

	for i := 1; i <= m; i++ {
		for j := 1; j <= n; j++ {
			if s1[i-1] == s2[j-1] {
				matrix[i][j] = matrix[i-1][j-1] + 1
				if matrix[i][j] > maxLen {
					maxLen = matrix[i][j]
					endPos = i
				}
			}
		}
	}

	if endPos > maxLen {
		return s1[endPos-maxLen : endPos]
	}
	return ""
}

func (sp *StringProcessor) EditDistance(s1, s2 string) int {
	m, n := len(s1), len(s2)
	dp := make([][]int, m+1)
	for i := range dp {
		dp[i] = make([]int, n+1)
	}

	for i := 0; i <= m; i++ {
		dp[i][0] = i
	}
	for j := 0; j <= n; j++ {
		dp[0][j] = j
	}

	for i := 1; i <= m; i++ {
		for j := 1; j <= n; j++ {
			if s1[i-1] == s2[j-1] {
				dp[i][j] = dp[i-1][j-1]
			} else {
				minVal := dp[i-1][j]
				if dp[i][j-1] < minVal {
					minVal = dp[i][j-1]
				}
				if dp[i-1][j-1] < minVal {
					minVal = dp[i-1][j-1]
				}
				dp[i][j] = 1 + minVal
			}
		}
	}

	return dp[m][n]
}

// ============================================================================
// Main Function
// ============================================================================

func main() {
	fmt.Println("Enhanced Medium Go Binary - Advanced Concurrency and Algorithms")
	fmt.Println("==============================================================")

	// Test Graph algorithms
	fmt.Println("\n--- Graph Algorithms ---")
	graph := &Graph{nodes: make(map[int]*GraphNode)}
	for i := 0; i < 6; i++ {
		graph.AddNode(i)
	}
	graph.AddEdge(0, 1)
	graph.AddEdge(0, 2)
	graph.AddEdge(1, 3)
	graph.AddEdge(2, 3)
	graph.AddEdge(3, 4)
	graph.AddEdge(4, 5)

	bfsResult := graph.BFS(0)
	fmt.Printf("BFS from node 0: %v\n", bfsResult)

	dfsResult := make([]int, 0)
	visited := make(map[int]bool)
	graph.DFS(0, visited, &dfsResult)
	fmt.Printf("DFS from node 0: %v\n", dfsResult)

	// Test Worker Pool
	fmt.Println("\n--- Worker Pool ---")
	pool := NewWorkerPool(4)
	pool.Start()

	for i := 0; i < 20; i++ {
		pool.Submit(&Task{ID: i, Value: i * 10})
	}
	pool.Stop()

	processed := 0
	for task := range pool.results {
		processed++
		if processed > 5 {
			break
		}
		fmt.Printf("Task %d: Value=%d\n", task.ID, task.Value)
	}

	// Test Container Interface
	fmt.Println("\n--- Container Interface ---")
	var container Container = &Stack{}
	container.Push(10)
	container.Push(20)
	container.Push(30)
	fmt.Printf("Stack size: %d\n", container.Size())

	// Test Advanced Sorting
	fmt.Println("\n--- Advanced Sorting ---")
	data := []int{64, 34, 25, 12, 22, 11, 90, 88, 45, 50}
	sorter := &Sorter{data: data}
	sorter.HeapSort()
	fmt.Printf("Heap sorted: %v\n", sorter.data)

	// Test String Processing
	fmt.Println("\n--- String Processing ---")
	sp := &StringProcessor{}
	positions := sp.FindAllOccurrences("hello world hello", "hello")
	fmt.Printf("Pattern positions: %v\n", positions)

	lcs := sp.LongestCommonSubstring("abcdef", "fbdamn")
	fmt.Printf("LCS: %s\n", lcs)

	editDist := sp.EditDistance("kitten", "sitting")
	fmt.Printf("Edit distance: %d\n", editDist)

	fmt.Println("\nEnhanced Go compilation successful!")
}
