// Medium C++ Binaries - Enhanced OOP and Templates
// Includes: Classes, Move semantics, Templates, Standard Library, Exception handling

#include <iostream>
#include <vector>
#include <map>
#include <string>
#include <algorithm>
#include <cmath>
#include <memory>
#include <utility>
#include <stdexcept>
#include <functional>

// ============================================================================
// Template Classes
// ============================================================================

template <typename T>
class Stack {
private:
    std::vector<T> elements;
    
public:
    Stack() = default;
    
    // Move constructor and assignment
    Stack(Stack&& other) noexcept : elements(std::move(other.elements)) {}
    Stack& operator=(Stack&& other) noexcept {
        elements = std::move(other.elements);
        return *this;
    }
    
    // Deleted copy constructor and assignment (move-only semantics)
    Stack(const Stack&) = delete;
    Stack& operator=(const Stack&) = delete;
    
    void push(T value) {
        elements.push_back(std::move(value));
    }
    
    T pop() {
        if (elements.empty()) throw std::runtime_error("Stack underflow");
        T value = std::move(elements.back());
        elements.pop_back();
        return value;
    }
    
    bool is_empty() const {
        return elements.empty();
    }
    
    size_t size() const {
        return elements.size();
    }
};

template <typename K, typename V>
class HashMap {
private:
    std::map<K, V> data;
    
public:
    HashMap() = default;
    
    // Move semantics
    HashMap(HashMap&& other) noexcept : data(std::move(other.data)) {}
    HashMap& operator=(HashMap&& other) noexcept {
        data = std::move(other.data);
        return *this;
    }
    
    void insert(K key, V value) {
        data[std::move(key)] = std::move(value);
    }
    
    V get(const K& key) const {
        auto it = data.find(key);
        if (it != data.end()) {
            return it->second;
        }
        return V();
    }
    
    bool contains(const K& key) const {
        return data.find(key) != data.end();
    }
    
    void remove(const K& key) {
        data.erase(key);
    }
    
    size_t size() const {
        return data.size();
    }
};

// ============================================================================
// Object-Oriented Classes
// ============================================================================

class Shape {
protected:
    double x, y;
    
public:
    Shape(double x = 0, double y = 0) : x(x), y(y) {}
    virtual ~Shape() = default;
    
    virtual double area() const = 0;
    virtual double perimeter() const = 0;
    
    double get_x() const { return x; }
    double get_y() const { return y; }
};

class Circle : public Shape {
private:
    double radius;
    
public:
    Circle(double x = 0, double y = 0, double r = 1.0)
        : Shape(x, y), radius(r) {}
    
    double area() const override {
        return 3.14159 * radius * radius;
    }
    
    double perimeter() const override {
        return 2.0 * 3.14159 * radius;
    }
    
    double get_radius() const { return radius; }
};

class Rectangle : public Shape {
private:
    double width, height;
    
public:
    Rectangle(double x = 0, double y = 0, double w = 1.0, double h = 1.0)
        : Shape(x, y), width(w), height(h) {}
    
    double area() const override {
        return width * height;
    }
    
    double perimeter() const override {
        return 2.0 * (width + height);
    }
    
    double get_width() const { return width; }
    double get_height() const { return height; }
};

class Triangle : public Shape {
private:
    double a, b, c;
    
public:
    Triangle(double x = 0, double y = 0, double a = 1.0, double b = 1.0, double c = 1.0)
        : Shape(x, y), a(a), b(b), c(c) {}
    
    double area() const override {
        double s = (a + b + c) / 2.0;
        return std::sqrt(s * (s - a) * (s - b) * (s - c));
    }
    
    double perimeter() const override {
        return a + b + c;
    }
};

// ============================================================================
// Tree Structure
// ============================================================================

template <typename T>
class TreeNode {
public:
    T data;
    std::vector<std::shared_ptr<TreeNode<T>>> children;
    
    TreeNode(const T& val) : data(val) {}
    
    void add_child(std::shared_ptr<TreeNode<T>> child) {
        children.push_back(child);
    }
};

template <typename T>
class BinarySearchTree {
private:
    struct Node {
        T data;
        std::shared_ptr<Node> left;
        std::shared_ptr<Node> right;
        
        Node(const T& val) : data(val), left(nullptr), right(nullptr) {}
    };
    
    std::shared_ptr<Node> root;
    
    std::shared_ptr<Node> insert_recursive(std::shared_ptr<Node> node, const T& value) {
        if (!node) {
            return std::make_shared<Node>(value);
        }
        
        if (value < node->data) {
            node->left = insert_recursive(node->left, value);
        } else if (value > node->data) {
            node->right = insert_recursive(node->right, value);
        }
        
        return node;
    }
    
    bool search_recursive(std::shared_ptr<Node> node, const T& value) const {
        if (!node) return false;
        
        if (value == node->data) return true;
        if (value < node->data) return search_recursive(node->left, value);
        return search_recursive(node->right, value);
    }
    
public:
    BinarySearchTree() : root(nullptr) {}
    
    void insert(const T& value) {
        root = insert_recursive(root, value);
    }
    
    bool search(const T& value) const {
        return search_recursive(root, value);
    }
};

// ============================================================================
// String Processing with STL and Algorithms
// ============================================================================

class StringUtils {
public:
    static std::string reverse_string(const std::string& str) {
        std::string reversed = str;
        std::reverse(reversed.begin(), reversed.end());
        return reversed;
    }
    
    static bool is_palindrome(const std::string& str) {
        std::string reversed = reverse_string(str);
        return str == reversed;
    }
    
    static std::vector<std::string> split_string(const std::string& str, char delimiter) {
        std::vector<std::string> result;
        std::string current;
        
        for (char c : str) {
            if (c == delimiter) {
                if (!current.empty()) {
                    result.push_back(current);
                    current.clear();
                }
            } else {
                current += c;
            }
        }
        
        if (!current.empty()) {
            result.push_back(current);
        }
        
        return result;
    }
    
    static int find_pattern(const std::string& text, const std::string& pattern) {
        size_t pos = text.find(pattern);
        return (pos != std::string::npos) ? static_cast<int>(pos) : -1;
    }
    
    static std::string to_uppercase(const std::string& str) {
        std::string result = str;
        std::transform(result.begin(), result.end(), result.begin(), ::toupper);
        return result;
    }
    
    static std::string to_lowercase(const std::string& str) {
        std::string result = str;
        std::transform(result.begin(), result.end(), result.begin(), ::tolower);
        return result;
    }
};

// ============================================================================
// Advanced Algorithm Class
// ============================================================================

class Algorithm {
public:
    static int binary_search(const std::vector<int>& arr, int target) {
        int left = 0, right = static_cast<int>(arr.size()) - 1;
        
        while (left <= right) {
            int mid = left + (right - left) / 2;
            
            if (arr[mid] == target) return mid;
            if (arr[mid] < target) {
                left = mid + 1;
            } else {
                right = mid - 1;
            }
        }
        
        return -1;
    }
    
    static std::vector<int> merge_arrays(const std::vector<int>& a, const std::vector<int>& b) {
        std::vector<int> result;
        result.reserve(a.size() + b.size());
        
        auto it_a = a.begin();
        auto it_b = b.begin();
        
        while (it_a != a.end() || it_b != b.end()) {
            if (it_a == a.end()) {
                result.insert(result.end(), it_b, b.end());
                break;
            } else if (it_b == b.end()) {
                result.insert(result.end(), it_a, a.end());
                break;
            } else if (*it_a <= *it_b) {
                result.push_back(*it_a++);
            } else {
                result.push_back(*it_b++);
            }
        }
        
        return result;
    }
    
    // Heap sort using templates
    template <typename T>
    static void heap_sort(std::vector<T>& arr) {
        if (arr.size() <= 1) return;
        
        auto heapify = [&arr](size_t n, size_t i) {
            size_t largest = i;
            size_t left = 2 * i + 1;
            size_t right = 2 * i + 2;
            
            if (left < n && arr[left] > arr[largest])
                largest = left;
            if (right < n && arr[right] > arr[largest])
                largest = right;
            
            if (largest != i) {
                std::swap(arr[i], arr[largest]);
                heapify(n, largest);
            }
        };
        
        for (int i = static_cast<int>(arr.size()) / 2 - 1; i >= 0; i--)
            heapify(arr.size(), i);
        
        for (int i = static_cast<int>(arr.size()) - 1; i > 0; i--) {
            std::swap(arr[0], arr[i]);
            heapify(i, 0);
        }
    }
};

// ============================================================================
// Main Function
// ============================================================================

int main() {
    std::cout << "Enhanced Medium C++ Binary - OOP, Templates, and Move Semantics" << std::endl;
    std::cout << "==============================================================\n" << std::endl;
    
    try {
        // Test templates with move semantics
        std::cout << "--- Template Stack with Move Semantics ---" << std::endl;
        Stack<int> stack;
        stack.push(10);
        stack.push(20);
        stack.push(30);
        std::cout << "Stack size: " << stack.size() << std::endl;
        
        // Test HashMap
        std::cout << "\n--- HashMap ---" << std::endl;
        HashMap<std::string, int> map;
        map.insert("one", 1);
        map.insert("two", 2);
        map.insert("three", 3);
        std::cout << "HashMap size: " << map.size() << std::endl;
        
        // Test inheritance and polymorphism
        std::cout << "\n--- Shape Hierarchy ---" << std::endl;
        Circle circle(0, 0, 5);
        Rectangle rect(0, 0, 4, 6);
        Triangle tri(0, 0, 3, 4, 5);
        
        std::cout << "Circle area: " << circle.area() << std::endl;
        std::cout << "Rectangle perimeter: " << rect.perimeter() << std::endl;
        
        // Test BST
        std::cout << "\n--- Binary Search Tree ---" << std::endl;
        BinarySearchTree<int> bst;
        bst.insert(50);
        bst.insert(30);
        bst.insert(70);
        std::cout << "BST search 30: " << (bst.search(30) ? "Found" : "Not found") << std::endl;
        
        // Test string utilities
        std::cout << "\n--- String Processing ---" << std::endl;
        std::string text = "Hello World";
        std::cout << "Original: " << text << std::endl;
        std::cout << "Uppercase: " << StringUtils::to_uppercase(text) << std::endl;
        
        std::string palindrome_test = "racecar";
        std::cout << "Is 'racecar' palindrome: " << (StringUtils::is_palindrome(palindrome_test) ? "Yes" : "No") << std::endl;
        
        // Test algorithms
        std::cout << "\n--- Algorithm Tests ---" << std::endl;
        std::vector<int> sorted_arr = {1, 3, 5, 7, 9, 11, 13};
        int found = Algorithm::binary_search(sorted_arr, 7);
        std::cout << "Binary search for 7: " << (found >= 0 ? "Found" : "Not found") << std::endl;
        
        // Test heap sort with lambdas
        std::cout << "\n--- Heap Sort ---" << std::endl;
        std::vector<int> data = {64, 34, 25, 12, 22, 11, 90, 88};
        Algorithm::heap_sort(data);
        std::cout << "Sorted successfully" << std::endl;
        
        std::cout << "\n✓ Enhanced C++ compilation successful!" << std::endl;
        
    } catch (const std::exception& e) {
        std::cerr << "Exception: " << e.what() << std::endl;
        return 1;
    }
    
    return 0;
}
