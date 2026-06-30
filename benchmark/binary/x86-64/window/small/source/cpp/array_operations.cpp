#include <iostream>
#include <vector>
#include <algorithm>
#include <numeric>

// Array operations template class
template <typename T>
class ArrayOps {
private:
    std::vector<T> data;
    
public:
    ArrayOps(const std::vector<T>& arr) : data(arr) {}
    
    T findMax() const {
        return *std::max_element(data.begin(), data.end());
    }
    
    T findMin() const {
        return *std::min_element(data.begin(), data.end());
    }
    
    T sum() const {
        return std::accumulate(data.begin(), data.end(), T(0));
    }
    
    double average() const {
        if (data.empty()) return 0.0;
        return static_cast<double>(sum()) / data.size();
    }
};

// Binary search implementation
class BinarySearch {
public:
    static int search(const std::vector<int>& arr, int target) {
        int left = 0;
        int right = arr.size() - 1;
        
        while (left <= right) {
            int mid = left + (right - left) / 2;
            
            if (arr[mid] == target) {
                return mid;
            }
            else if (arr[mid] < target) {
                left = mid + 1;
            }
            else {
                right = mid - 1;
            }
        }
        
        return -1;
    }
};

// Linear search wrapper
class LinearSearch {
public:
    static int search(const std::vector<int>& arr, int target) {
        auto it = std::find(arr.begin(), arr.end(), target);
        if (it != arr.end()) {
            return std::distance(arr.begin(), it);
        }
        return -1;
    }
};

// Merge sorted arrays
class MergeArrays {
public:
    static std::vector<int> merge(const std::vector<int>& arr1, 
                                   const std::vector<int>& arr2) {
        std::vector<int> result;
        result.reserve(arr1.size() + arr2.size());
        
        std::merge(arr1.begin(), arr1.end(),
                  arr2.begin(), arr2.end(),
                  std::back_inserter(result));
        
        return result;
    }
};

// Rotate array
class RotateArray {
public:
    static void rotate(std::vector<int>& arr, int k) {
        if (arr.empty() || k == 0) return;
        k = k % arr.size();
        std::rotate(arr.begin(), arr.begin() + (arr.size() - k), arr.end());
    }
};

// Remove duplicates
class RemoveDuplicates {
public:
    static int remove(std::vector<int>& arr) {
        auto it = std::unique(arr.begin(), arr.end());
        arr.erase(it, arr.end());
        return arr.size();
    }
};

// Sorting algorithms template
template <typename T>
class SortAlgorithms {
public:
    // Bubble sort
    static void bubbleSort(std::vector<T>& arr) {
        for (size_t i = 0; i < arr.size() - 1; i++) {
            for (size_t j = 0; j < arr.size() - i - 1; j++) {
                if (arr[j] > arr[j + 1]) {
                    std::swap(arr[j], arr[j + 1]);
                }
            }
        }
    }
    
    // Insertion sort
    static void insertionSort(std::vector<T>& arr) {
        for (size_t i = 1; i < arr.size(); i++) {
            T key = arr[i];
            int j = i - 1;
            
            while (j >= 0 && arr[j] > key) {
                arr[j + 1] = arr[j];
                j--;
            }
            arr[j + 1] = key;
        }
    }
    
    // Quick sort helper
    static int partition(std::vector<T>& arr, int low, int high) {
        T pivot = arr[high];
        int i = low - 1;
        
        for (int j = low; j < high; j++) {
            if (arr[j] < pivot) {
                i++;
                std::swap(arr[i], arr[j]);
            }
        }
        
        std::swap(arr[i + 1], arr[high]);
        return i + 1;
    }
    
    static void quickSort(std::vector<T>& arr, int low, int high) {
        if (low < high) {
            int pi = partition(arr, low, high);
            quickSort(arr, low, pi - 1);
            quickSort(arr, pi + 1, high);
        }
    }
};

// 2D array operations
class Matrix2DOperations {
public:
    static int sum(const std::vector<std::vector<int>>& matrix) {
        int total = 0;
        for (const auto& row : matrix) {
            total += std::accumulate(row.begin(), row.end(), 0);
        }
        return total;
    }
    
    static std::vector<std::vector<int>> transpose(
        const std::vector<std::vector<int>>& matrix) {
        if (matrix.empty()) return {};
        
        int rows = matrix.size();
        int cols = matrix[0].size();
        
        std::vector<std::vector<int>> result(cols, std::vector<int>(rows));
        
        for (int i = 0; i < rows; i++) {
            for (int j = 0; j < cols; j++) {
                result[j][i] = matrix[i][j];
            }
        }
        
        return result;
    }
};

int main() {
    // Test ArrayOps template
    std::vector<int> arr = {3, 7, 2, 9, 1, 5};
    ArrayOps<int> ops(arr);
    int max_val = ops.findMax();
    int min_val = ops.findMin();
    int sum = ops.sum();
    double avg = ops.average();
    
    // Test sorting algorithms
    std::vector<int> arr_bubble = {5, 2, 8, 1, 9};
    std::vector<int> arr_insertion = {5, 2, 8, 1, 9};
    std::vector<int> arr_quick = {5, 2, 8, 1, 9};
    
    SortAlgorithms<int>::bubbleSort(arr_bubble);
    SortAlgorithms<int>::insertionSort(arr_insertion);
    SortAlgorithms<int>::quickSort(arr_quick, 0, arr_quick.size() - 1);
    
    // Test binary search
    std::vector<int> sorted_arr = {1, 2, 3, 4, 5, 6, 7, 8, 9};
    int search_result = BinarySearch::search(sorted_arr, 5);
    
    // Test merge arrays
    std::vector<int> arr1 = {1, 3, 5};
    std::vector<int> arr2 = {2, 4, 6};
    std::vector<int> merged = MergeArrays::merge(arr1, arr2);
    
    // Test rotate array
    std::vector<int> arr_rotate = {1, 2, 3, 4, 5};
    RotateArray::rotate(arr_rotate, 2);
    
    // Test remove duplicates
    std::vector<int> arr_dup = {1, 1, 2, 2, 3, 3, 3};
    int new_len = RemoveDuplicates::remove(arr_dup);
    
    // Test 2D array operations
    std::vector<std::vector<int>> matrix = {
        {1, 2, 3},
        {4, 5, 6},
        {7, 8, 9}
    };
    int matrix_sum = Matrix2DOperations::sum(matrix);
    auto transposed = Matrix2DOperations::transpose(matrix);
    
    std::cout << "Array operations test completed: " 
              << max_val << " " << min_val << " " 
              << new_len << " " << matrix_sum << std::endl;
    
    return 0;
}
