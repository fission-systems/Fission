type CompareFunc = fn(i32, i32) -> i32;
type TransformFunc = fn(i32) -> i32;

fn compare_ascending(a: i32, b: i32) -> i32 {
    a - b
}

fn compare_descending(a: i32, b: i32) -> i32 {
    b - a
}

fn compare_abs_value(a: i32, b: i32) -> i32 {
    a.abs() - b.abs()
}

fn square(x: i32) -> i32 {
    x * x
}

fn cube(x: i32) -> i32 {
    x * x * x
}

fn negate(x: i32) -> i32 {
    -x
}

struct StringUtils;

impl StringUtils {
    fn reverse(s: &str) -> String {
        s.chars().rev().collect()
    }

    fn to_upper(s: &str) -> String {
        s.to_uppercase()
    }

    fn to_lower(s: &str) -> String {
        s.to_lowercase()
    }

    fn compare(s1: &str, s2: &str) -> i32 {
        if s1 < s2 {
            -1
        } else if s1 > s2 {
            1
        } else {
            0
        }
    }

    fn concatenate(s1: &str, s2: &str) -> String {
        format!("{}{}", s1, s2)
    }

    fn find_char(s: &str, ch: char) -> i32 {
        for (i, c) in s.chars().enumerate() {
            if c == ch {
                return i as i32;
            }
        }
        -1
    }
}

struct ArrayProcessor {
    data: Vec<i32>,
}

impl ArrayProcessor {
    fn new(data: Vec<i32>) -> Self {
        ArrayProcessor { data }
    }

    fn apply_transform(&mut self, f: TransformFunc) {
        self.data = self.data.iter().map(|&x| f(x)).collect();
    }

    fn custom_sort(&mut self, cmp: CompareFunc) {
        self.data.sort_by(|a, b| {
            let result = cmp(*a, *b);
            if result < 0 {
                std::cmp::Ordering::Less
            } else if result > 0 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
    }

    fn get_data(&self) -> Vec<i32> {
        self.data.clone()
    }
}

fn filter_even(arr: &[i32]) -> Vec<i32> {
    arr.iter().filter(|&&x| x % 2 == 0).copied().collect()
}

fn map_square(arr: &[i32]) -> Vec<i32> {
    arr.iter().map(|&x| x * x).collect()
}

fn fold(arr: &[i32], initial: i32) -> i32 {
    arr.iter().fold(initial, |acc, &x| acc + x)
}

fn for_each(arr: &[i32], mut f: impl FnMut(i32)) {
    for &x in arr {
        f(x);
    }
}

fn main() {
    // Test function pointers with sorting
    let mut arr1 = vec![3, 1, 4, 1, 5, 9, 2, 6];
    let mut arr2 = arr1.clone();

    let mut processor1 = ArrayProcessor::new(arr1);
    processor1.custom_sort(compare_ascending);

    let mut processor2 = ArrayProcessor::new(arr2);
    processor2.custom_sort(compare_descending);

    // Test transform
    let mut square_arr = ArrayProcessor::new(vec![1, 2, 3, 4, 5]);
    square_arr.apply_transform(square);

    // Test string operations
    let str1 = "Hello";
    let str2 = "World";

    let reversed = StringUtils::reverse(str1);
    let upper = StringUtils::to_upper(str1);
    let lower = StringUtils::to_lower(&upper);
    let cmp_result = StringUtils::compare(str1, str2);
    let concat = StringUtils::concatenate(str1, str2);
    let find_result = StringUtils::find_char("Hello World", 'o');

    // Test functional operations
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
    let even = filter_even(&data);
    let squared = map_square(&data);
    let fold_result = fold(&data, 0);

    // Test callbacks with closures
    let mut callback_sum = 0;
    for_each(&processor1.get_data(), |val| {
        callback_sum += val;
    });

    println!(
        "Function pointers and strings test: {} {} {}",
        reversed.len(),
        find_result,
        fold_result
    );
    println!("String ops: {} {}", upper, concat);
    println!(
        "Even count: {}, Squared count: {}",
        even.len(),
        squared.len()
    );
    println!("Callback sum: {}, Compare: {}", callback_sum, cmp_result);
}
