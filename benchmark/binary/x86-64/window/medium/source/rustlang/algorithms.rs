// Enhanced Medium Rust Binary - Advanced Ownership, Traits, Lifetimes, and Unsafe

use std::collections::HashMap;
use std::cmp::Ordering;

// ============================================================================
// Advanced Trait System
// ============================================================================

trait Drawable {
    fn draw(&self) -> String;
    fn area(&self) -> f64;
}

trait Comparable {
    fn compare(&self, other: &Self) -> Ordering;
}

trait Serializable {
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> Result<Self, String> where Self: Sized;
}

// ============================================================================
// Result and Option Handling
// ============================================================================

enum GraphError {
    NodeNotFound,
    EdgeNotFound,
    InvalidWeight,
}

type GraphResult<T> = Result<T, GraphError>;

// ============================================================================
// Graph with Error Handling
// ============================================================================

struct Graph {
    nodes: Vec<i32>,
    edges: Vec<(usize, usize, i32)>,
}

impl Graph {
    fn new() -> Self {
        Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn add_node(&mut self, value: i32) {
        self.nodes.push(value);
    }

    fn add_edge(&mut self, from: usize, to: usize, weight: i32) -> GraphResult<()> {
        if weight < 0 {
            return Err(GraphError::InvalidWeight);
        }
        if from >= self.nodes.len() || to >= self.nodes.len() {
            return Err(GraphError::NodeNotFound);
        }
        self.edges.push((from, to, weight));
        Ok(())
    }

    fn bfs(&self, start: usize) -> GraphResult<Vec<usize>> {
        if start >= self.nodes.len() {
            return Err(GraphError::NodeNotFound);
        }

        let mut visited = vec![false; self.nodes.len()];
        let mut queue = Vec::new();
        let mut result = Vec::new();

        queue.push(start);
        visited[start] = true;

        while let Some(node) = queue.pop() {
            result.push(node);

            for &(from, to, _) in &self.edges {
                if from == node && !visited[to] {
                    visited[to] = true;
                    queue.push(to);
                }
            }
        }

        Ok(result)
    }
}

// ============================================================================
// Generic Structures with Lifetimes
// ============================================================================

#[derive(Clone)]
struct Point<T: Clone> {
    x: T,
    y: T,
}

impl<T: Clone> Point<T> {
    fn new(x: T, y: T) -> Self {
        Point { x, y }
    }

    fn get_x(&self) -> &T {
        &self.x
    }

    fn get_y(&self) -> &T {
        &self.y
    }
}

// String reference with lifetime
struct NamedPoint<'a> {
    name: &'a str,
    x: f64,
    y: f64,
}

impl<'a> NamedPoint<'a> {
    fn new(name: &'a str, x: f64, y: f64) -> Self {
        NamedPoint { name, x, y }
    }

    fn get_name(&self) -> &'a str {
        self.name
    }
}

// ============================================================================
// Shape Implementations with Advanced Traits
// ============================================================================

#[derive(Clone)]
struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

impl Drawable for Circle {
    fn draw(&self) -> String {
        format!("Drawing circle at ({}, {}) with radius {}", self.x, self.y, self.radius)
    }

    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
}

#[derive(Clone)]
struct Rectangle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Drawable for Rectangle {
    fn draw(&self) -> String {
        format!("Drawing rectangle at ({}, {}) with size {}x{}", self.x, self.y, self.width, self.height)
    }

    fn area(&self) -> f64 {
        self.width * self.height
    }
}

// ============================================================================
// Unsafe Code for Low-Level Operations
// ============================================================================

unsafe fn dangerous_pointer_arithmetic(ptr: *mut i32, offset: usize) -> *mut i32 {
    ptr.add(offset)
}

fn safe_buffer_manipulation(data: &mut [i32], offset: usize) -> Option<&mut i32> {
    if offset < data.len() {
        Some(&mut data[offset])
    } else {
        None
    }
}

// ============================================================================
// Collection Types with Generic Constraints
// ============================================================================

struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::new() }
    }

    fn push(&mut self, item: T) {
        self.items.push(item);
    }

    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn size(&self) -> usize {
        self.items.len()
    }
}

// ============================================================================
// Tree Structures with Advanced Patterns
// ============================================================================

#[derive(Clone)]
enum TreeNode<T: Clone> {
    Node {
        value: T,
        left: Option<Box<TreeNode<T>>>,
        right: Option<Box<TreeNode<T>>>,
    },
    Empty,
}

impl<T: Clone + Ord> TreeNode<T> {
    fn new(value: T) -> Self {
        TreeNode::Node {
            value,
            left: None,
            right: None,
        }
    }

    fn insert(&mut self, value: T) {
        match self {
            TreeNode::Node { value: v, left, right } => {
                if value < *v {
                    match left {
                        Some(left_node) => left_node.insert(value),
                        None => {
                            *left = Some(Box::new(TreeNode::new(value)));
                        }
                    }
                } else {
                    match right {
                        Some(right_node) => right_node.insert(value),
                        None => {
                            *right = Some(Box::new(TreeNode::new(value)));
                        }
                    }
                }
            }
            TreeNode::Empty => {}
        }
    }

    fn search(&self, value: &T) -> bool {
        match self {
            TreeNode::Node { value: v, left, right } => {
                if value == v {
                    true
                } else if value < v {
                    match left {
                        Some(left_node) => left_node.search(value),
                        None => false,
                    }
                } else {
                    match right {
                        Some(right_node) => right_node.search(value),
                        None => false,
                    }
                }
            }
            TreeNode::Empty => false,
        }
    }

    fn collect_sorted(&self, result: &mut Vec<T>) {
        match self {
            TreeNode::Node { value, left, right } => {
                if let Some(l) = left {
                    l.collect_sorted(result);
                }
                result.push(value.clone());
                if let Some(r) = right {
                    r.collect_sorted(result);
                }
            }
            TreeNode::Empty => {}
        }
    }
}

// ============================================================================
// Sorting Algorithms
// ============================================================================

struct Sorter;

impl Sorter {
    fn quick_sort<T: Ord>(arr: &mut [T]) {
        if arr.len() <= 1 {
            return;
        }

        let pivot_idx = Self::partition(arr);
        Self::quick_sort(&mut arr[..pivot_idx]);
        Self::quick_sort(&mut arr[pivot_idx + 1..]);
    }

    fn partition<T: Ord>(arr: &mut [T]) -> usize {
        let len = arr.len();
        let pivot_idx = len - 1;
        let mut store_idx = 0;

        for i in 0..pivot_idx {
            if arr[i] < arr[pivot_idx] {
                arr.swap(i, store_idx);
                store_idx += 1;
            }
        }

        arr.swap(store_idx, pivot_idx);
        store_idx
    }

    fn merge_sort<T: Ord + Clone>(arr: &[T]) -> Vec<T> {
        if arr.len() <= 1 {
            return arr.to_vec();
        }

        let mid = arr.len() / 2;
        let left = Self::merge_sort(&arr[..mid]);
        let right = Self::merge_sort(&arr[mid..]);

        Self::merge(left, right)
    }

    fn merge<T: Ord + Clone>(mut left: Vec<T>, mut right: Vec<T>) -> Vec<T> {
        let mut result = Vec::new();

        loop {
            match (left.first(), right.first()) {
                (Some(l), Some(r)) if l <= r => {
                    result.push(left.remove(0));
                }
                (Some(_), Some(_)) => {
                    result.push(right.remove(0));
                }
                (Some(_), None) => {
                    result.extend(left);
                    break;
                }
                (None, Some(_)) => {
                    result.extend(right);
                    break;
                }
                (None, None) => break,
            }
        }

        result
    }
}

// ============================================================================
// String Processing with Iterators
// ============================================================================

struct StringUtils;

impl StringUtils {
    fn reverse(s: &str) -> String {
        s.chars().rev().collect()
    }

    fn is_palindrome(s: &str) -> bool {
        let reversed = Self::reverse(s);
        s == reversed
    }

    fn find_pattern(text: &str, pattern: &str) -> Option<usize> {
        text.find(pattern)
    }

    fn find_all_patterns(text: &str, pattern: &str) -> Vec<usize> {
        text.match_indices(pattern).map(|(i, _)| i).collect()
    }

    fn split_and_process<F>(s: &str, delimiter: char, processor: F) -> Vec<String>
    where
        F: Fn(&str) -> String,
    {
        s.split(delimiter).map(processor).collect()
    }

    fn character_frequency(s: &str) -> HashMap<char, usize> {
        let mut freq = HashMap::new();
        for c in s.chars() {
            *freq.entry(c).or_insert(0) += 1;
        }
        freq
    }
}

// ============================================================================
// Main Function
// ============================================================================

fn main() {
    println!("Enhanced Medium Rust Binary - Advanced Ownership and Traits");
    println!("===========================================================\n");

    // Test Graph with Error Handling
    println!("--- Graph Algorithms with Error Handling ---");
    let mut graph = Graph::new();
    graph.add_node(0);
    graph.add_node(1);
    graph.add_node(2);
    graph.add_edge(0, 1, 5).expect("Failed to add edge");
    graph.add_edge(1, 2, 3).expect("Failed to add edge");

    match graph.bfs(0) {
        Ok(result) => println!("BFS result: {:?}", result),
        Err(_) => println!("BFS error"),
    }

    // Test invalid weight
    match graph.add_edge(0, 1, -1) {
        Ok(_) => println!("Edge added"),
        Err(GraphError::InvalidWeight) => println!("Invalid weight detected"),
        Err(_) => println!("Other error"),
    }

    // Test Generic Point with Lifetimes
    println!("\n--- Generic Structures and Lifetimes ---");
    let point_i32 = Point::new(10, 20);
    println!("Point X: {}", point_i32.get_x());

    let named_point = NamedPoint::new("origin", 0.0, 0.0);
    println!("Named point: {}", named_point.get_name());

    // Test Traits
    println!("\n--- Traits ---");
    let circle = Circle { x: 0.0, y: 0.0, radius: 5.0 };
    let rect = Rectangle { x: 0.0, y: 0.0, width: 4.0, height: 6.0 };

    println!("{}", circle.draw());
    println!("{}", rect.draw());
    println!("Circle area: {}", circle.area());
    println!("Rectangle area: {}", rect.area());

    // Test Stack with Option
    println!("\n--- Stack with Option/Result ---");
    let mut stack = Stack::new();
    stack.push(10);
    stack.push(20);
    stack.push(30);
    println!("Stack size: {}", stack.size());

    while let Some(val) = stack.pop() {
        println!("Popped: {}", val);
    }

    // Test Tree
    println!("\n--- Binary Search Tree ---");
    let mut tree = TreeNode::new(50);
    tree.insert(30);
    tree.insert(70);
    tree.insert(20);
    tree.insert(40);
    println!("Tree search 30: {}", tree.search(&30));
    println!("Tree search 100: {}", tree.search(&100));

    let mut sorted = Vec::new();
    tree.collect_sorted(&mut sorted);
    println!("Sorted tree: {:?}", sorted);

    // Test Sorting
    println!("\n--- Sorting Algorithms ---");
    let data = vec![64, 34, 25, 12, 22, 11, 90, 88];
    let sorted = Sorter::merge_sort(&data);
    println!("Merge sorted: {:?}", sorted);

    // Test String Processing
    println!("\n--- String Processing with Iterators ---");
    let text = "hello world";
    let reversed = StringUtils::reverse(text);
    let is_pal = StringUtils::is_palindrome("racecar");
    let positions = StringUtils::find_all_patterns("hello world hello", "hello");

    println!("Reversed: {}", reversed);
    println!("Is 'racecar' palindrome: {}", is_pal);
    println!("All 'hello' positions: {:?}", positions);

    // Test character frequency
    let freq = StringUtils::character_frequency("aabbbcccc");
    println!("Character frequency: {:?}", freq);

    // Test Unsafe Code
    println!("\n--- Unsafe Operations ---");
    let mut data = vec![1, 2, 3, 4, 5];
    if let Some(elem) = safe_buffer_manipulation(&mut data, 2) {
        *elem = 99;
        println!("Modified element: {}", elem);
    }

    // Test Higher-Order Functions
    println!("\n--- Higher-Order Functions ---");
    let words: Vec<String> = StringUtils::split_and_process("hello,world,rust", ',', |s| {
        StringUtils::reverse(s)
    });
    println!("Processed words: {:?}", words);

    println!("\n✓ Enhanced Rust compilation successful!");
}
