fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

fn fibonacci(n: i32) -> i32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn sum_array(arr: &[i32]) -> i32 {
    arr.iter().sum()
}

fn process_code(code: i32) -> i32 {
    match code {
        1 => 10,
        2 => 20,
        3 => 30,
        _ => 0,
    }
}

fn fill_matrix(rows: usize, cols: usize, value: i32) -> Vec<Vec<i32>> {
    vec![vec![value; cols]; rows]
}

fn swap(a: &mut i32, b: &mut i32) {
    let temp = *a;
    *a = *b;
    *b = temp;
}

fn main() {
    let x = 5;
    let y = 10;

    let result1 = add(x, y);
    let result2 = max(x, y);
    let result3 = fibonacci(10);

    let arr = vec![1, 2, 3, 4, 5];
    let arr_sum = sum_array(&arr);

    let code_result = process_code(2);

    let mut a = x;
    let mut b = y;
    swap(&mut a, &mut b);

    let matrix = fill_matrix(3, 3, 5);

    println!("Results: {} {} {} {} {}", result1, result2, result3, arr_sum, code_result);
    println!("Matrix size: {} x {}", matrix.len(), matrix[0].len());
}
