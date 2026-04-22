#include <iostream>
#include <string>
#include <vector>
#include <algorithm>
#include <functional>

// Function pointer typedef
typedef int (*CompareFunc)(int, int);
typedef int (*TransformFunc)(int);

// Comparison functions
int compareAscending(int a, int b) {
    return a - b;
}

int compareDescending(int a, int b) {
    return b - a;
}

int compareAbsValue(int a, int b) {
    return std::abs(a) - std::abs(b);
}

// Transform functions
int square(int x) {
    return x * x;
}

int cube(int x) {
    return x * x * x;
}

int negate(int x) {
    return -x;
}

// String wrapper class
class StringUtils {
public:
    static std::string reverse(std::string str) {
        std::reverse(str.begin(), str.end());
        return str;
    }
    
    static std::string toUpper(std::string str) {
        std::transform(str.begin(), str.end(), str.begin(), 
                      [](unsigned char c) { return std::toupper(c); });
        return str;
    }
    
    static std::string toLower(std::string str) {
        std::transform(str.begin(), str.end(), str.begin(), 
                      [](unsigned char c) { return std::tolower(c); });
        return str;
    }
    
    static int compare(const std::string& s1, const std::string& s2) {
        if (s1 < s2) return -1;
        if (s1 > s2) return 1;
        return 0;
    }
    
    static std::string concatenate(const std::string& s1, const std::string& s2) {
        return s1 + s2;
    }
    
    static size_t findChar(const std::string& str, char ch) {
        return str.find(ch);
    }
};

// Comparator class for custom sorting
class Comparator {
private:
    CompareFunc cmp;
    
public:
    Comparator(CompareFunc f) : cmp(f) {}
    
    bool operator()(int a, int b) const {
        return cmp(a, b) < 0;
    }
};

// Array processor with function pointers
class ArrayProcessorWithFP {
private:
    std::vector<int> data;
    
public:
    ArrayProcessorWithFP(const std::vector<int>& arr) : data(arr) {}
    
    void applyTransform(TransformFunc func) {
        std::transform(data.begin(), data.end(), data.begin(), func);
    }
    
    void sort(CompareFunc cmp) {
        std::sort(data.begin(), data.end(), 
                 [cmp](int a, int b) { return cmp(a, b) < 0; });
    }
    
    std::vector<int> getData() const {
        return data;
    }
};

// Lambda-based operations
class LambdaOperations {
public:
    static std::vector<int> filterEven(const std::vector<int>& arr) {
        std::vector<int> result;
        std::copy_if(arr.begin(), arr.end(), std::back_inserter(result),
                    [](int x) { return x % 2 == 0; });
        return result;
    }
    
    static std::vector<int> mapSquare(const std::vector<int>& arr) {
        std::vector<int> result;
        std::transform(arr.begin(), arr.end(), std::back_inserter(result),
                      [](int x) { return x * x; });
        return result;
    }
    
    static int fold(const std::vector<int>& arr, int initial) {
        int result = initial;
        for (int val : arr) {
            result += val;
        }
        return result;
    }
};

// Callback-based processor
class CallbackProcessor {
public:
    template <typename Callback>
    static void forEach(const std::vector<int>& arr, Callback cb) {
        for (int val : arr) {
            cb(val);
        }
    }
};

int main() {
    // Test function pointers with sorting
    std::vector<int> arr1 = {3, 1, 4, 1, 5, 9, 2, 6};
    std::vector<int> arr2 = arr1;
    std::vector<int> arr3 = arr1;
    
    // Sort using different comparators
    std::sort(arr1.begin(), arr1.end(), 
             [](int a, int b) { return compareAscending(a, b) < 0; });
    std::sort(arr2.begin(), arr2.end(), 
             [](int a, int b) { return compareDescending(a, b) < 0; });
    std::sort(arr3.begin(), arr3.end(), 
             [](int a, int b) { return compareAbsValue(a, b) < 0; });
    
    // Test transform functions
    std::vector<int> square_arr = {1, 2, 3, 4, 5};
    std::transform(square_arr.begin(), square_arr.end(), 
                  square_arr.begin(), square);
    
    // Test string operations
    std::string str1 = "Hello";
    std::string str2 = "World";
    
    std::string reversed = StringUtils::reverse(str1);
    std::string upper = StringUtils::toUpper(str1);
    std::string lower = StringUtils::toLower(upper);
    int cmp_result = StringUtils::compare(str1, str2);
    std::string concat = StringUtils::concatenate(str1, str2);
    size_t find_result = StringUtils::findChar("Hello World", 'o');
    
    // Test lambda operations
    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8, 9};
    std::vector<int> even = LambdaOperations::filterEven(data);
    std::vector<int> squared = LambdaOperations::mapSquare(data);
    int fold_result = LambdaOperations::fold(data, 0);
    
    // Test callbacks
    int callback_sum = 0;
    CallbackProcessor::forEach(arr1, 
        [&callback_sum](int val) { callback_sum += val; });
    
    // Test ArrayProcessorWithFP
    ArrayProcessorWithFP processor(data);
    std::vector<int> copy = processor.getData();
    processor.applyTransform(square);
    processor.sort(compareAscending);
    
    std::cout << "Function pointers and strings test completed: " 
              << arr1.size() << " " << find_result << " " 
              << fold_result << " " << callback_sum << std::endl;
    
    return 0;
}
