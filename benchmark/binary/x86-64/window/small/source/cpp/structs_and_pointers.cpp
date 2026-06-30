#include <iostream>
#include <string>
#include <vector>
#include <memory>

// Structure-like class
class Record {
public:
    int id;
    int value;
    std::string name;
    
    Record(int id, int value, const std::string& name)
        : id(id), value(value), name(name) {}
    
    int calculate() const {
        return id + value;
    }
};

// Linked list node class
class Node {
public:
    int data;
    std::shared_ptr<Node> next;
    std::shared_ptr<Node> prev;
    
    Node(int data) : data(data), next(nullptr), prev(nullptr) {}
};

// Linked list container
class LinkedList {
private:
    std::shared_ptr<Node> head;
    
public:
    LinkedList() : head(nullptr) {}
    
    void insertAtHead(int data) {
        auto new_node = std::make_shared<Node>(data);
        if (head) {
            new_node->next = head;
            head->prev = new_node;
        }
        head = new_node;
    }
    
    int sum() const {
        int total = 0;
        auto current = head;
        while (current) {
            total += current->data;
            current = current->next;
        }
        return total;
    }
    
    int length() const {
        int count = 0;
        auto current = head;
        while (current) {
            count++;
            current = current->next;
        }
        return count;
    }
};

// Point class using template
template <typename T>
class Point {
private:
    T x, y, z;
    
public:
    Point(T x = 0, T y = 0, T z = 0) : x(x), y(y), z(z) {}
    
    T distanceFromOrigin() const {
        return x * x + y * y + z * z;
    }
    
    void translate(T dx, T dy, T dz) {
        x += dx;
        y += dy;
        z += dz;
    }
};

// Container class holding multiple records
class RecordManager {
private:
    std::vector<Record> records;
    
public:
    void addRecord(const Record& rec) {
        records.push_back(rec);
    }
    
    int getTotalValue() const {
        int total = 0;
        for (const auto& rec : records) {
            total += rec.value;
        }
        return total;
    }
    
    int count() const {
        return records.size();
    }
};

// Pointer arithmetic with vectors
class VectorPointerOps {
private:
    std::vector<int> data;
    
public:
    VectorPointerOps(const std::vector<int>& arr) : data(arr) {}
    
    int sumViaPointers() const {
        int sum = 0;
        for (const auto& val : data) {
            sum += val;
        }
        return sum;
    }
    
    void modifyViaReference(int index, int value) {
        if (index >= 0 && index < static_cast<int>(data.size())) {
            data[index] = value;
        }
    }
};

// Exception handling
class ManagedResource {
private:
    int value;
    
public:
    ManagedResource(int val) : value(val) {
        if (val < 0) {
            throw std::invalid_argument("Value must be non-negative");
        }
    }
    
    int getValue() const { return value; }
    
    void setValue(int val) {
        if (val < 0) {
            throw std::out_of_range("Value out of range");
        }
        value = val;
    }
};

int main() {
    try {
        // Test Record class
        Record rec1(1, 100, "Alice");
        Record rec2(2, 200, "Bob");
        int calc1 = rec1.calculate();
        int calc2 = rec2.calculate();
        
        // Test linked list
        LinkedList list;
        list.insertAtHead(10);
        list.insertAtHead(20);
        list.insertAtHead(30);
        list.insertAtHead(40);
        int list_sum = list.sum();
        int list_len = list.length();
        
        // Test Point template
        Point<double> point1(1.0, 2.0, 3.0);
        double dist = point1.distanceFromOrigin();
        point1.translate(1.0, 1.0, 1.0);
        
        Point<int> point2(5, 5, 5);
        int int_dist = point2.distanceFromOrigin();
        
        // Test RecordManager
        RecordManager manager;
        manager.addRecord(rec1);
        manager.addRecord(rec2);
        int total_value = manager.getTotalValue();
        int record_count = manager.count();
        
        // Test pointer operations
        std::vector<int> arr = {1, 2, 3, 4, 5};
        VectorPointerOps vec_ops(arr);
        int vec_sum = vec_ops.sumViaPointers();
        vec_ops.modifyViaReference(0, 10);
        
        // Test exception handling
        ManagedResource res1(42);
        int res_value = res1.getValue();
        res1.setValue(100);
        
        std::cout << "Structs and pointers test completed: " 
                  << calc1 << " " << list_sum << " " << total_value << std::endl;
        
    } catch (const std::exception& e) {
        std::cerr << "Exception: " << e.what() << std::endl;
        return 1;
    }
    
    return 0;
}
