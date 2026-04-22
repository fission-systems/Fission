fn find_max(arr: &[i32]) -> i32 {
    if arr.is_empty() {
        0
    } else {
        *arr.iter().max().unwrap_or(&0)
    }
}

fn find_min(arr: &[i32]) -> i32 {
    if arr.is_empty() {
        0
    } else {
        *arr.iter().min().unwrap_or(&0)
    }
}

fn array_sum(arr: &[i32]) -> i32 {
    arr.iter().sum()
}

fn array_average(arr: &[i32]) -> f64 {
    if arr.is_empty() {
        0.0
    } else {
        arr.iter().sum::<i32>() as f64 / arr.len() as f64
    }
}

fn binary_search(arr: &[i32], target: i32) -> i32 {
    let mut left = 0;
    let mut right = arr.len() as i32 - 1;

    while left <= right {
        let mid = left + (right - left) / 2;
        let mid_idx = mid as usize;

        if arr[mid_idx] == target {
            return mid;
        } else if arr[mid_idx] < target {
            left = mid + 1;
        } else {
            right = mid - 1;
        }
    }

    -1
}

fn linear_search(arr: &[i32], target: i32) -> i32 {
    for (i, &val) in arr.iter().enumerate() {
        if val == target {
            return i as i32;
        }
    }
    -1
}

fn merge_arrays(arr1: &[i32], arr2: &[i32]) -> Vec<i32> {
    let mut result = Vec::with_capacity(arr1.len() + arr2.len());

    let mut i = 0;
    let mut j = 0;

    while i < arr1.len() && j < arr2.len() {
        if arr1[i] <= arr2[j] {
            result.push(arr1[i]);
            i += 1;
        } else {
            result.push(arr2[j]);
            j += 1;
        }
    }

    result.extend_from_slice(&arr1[i..]);
    result.extend_from_slice(&arr2[j..]);

    result
}

fn rotate_array(arr: &mut [i32], k: usize) {
    if arr.is_empty() || k == 0 {
        return;
    }

    let k = k % arr.len();

    arr.reverse();
    arr[..k].reverse();
    arr[k..].reverse();
}

fn remove_duplicates(arr: &mut Vec<i32>) -> Vec<i32> {
    if arr.is_empty() {
        return vec![];
    }

    arr.sort();
    arr.dedup();
    arr.clone()
}

fn bubble_sort(arr: &mut [i32]) {
    let n = arr.len();
    for i in 0..n - 1 {
        for j in 0..n - i - 1 {
            if arr[j] > arr[j + 1] {
                arr.swap(j, j + 1);
            }
        }
    }
}

fn insertion_sort(arr: &mut [i32]) {
    for i in 1..arr.len() {
        let key = arr[i];
        let mut j = i as i32 - 1;

        while j >= 0 && arr[j as usize] > key {
            arr[(j + 1) as usize] = arr[j as usize];
            j -= 1;
        }
        arr[(j + 1) as usize] = key;
    }
}

fn matrix_sum(matrix: &[Vec<i32>]) -> i32 {
    matrix
        .iter()
        .flat_map(|row| row.iter())
        .sum()
}

fn transpose_matrix(src: &[Vec<i32>]) -> Vec<Vec<i32>> {
    if src.is_empty() {
        return vec![];
    }

    let rows = src.len();
    let cols = src[0].len();

    let mut result = vec![vec![0; rows]; cols];

    for i in 0..rows {
        for j in 0..cols {
            result[j][i] = src[i][j];
        }
    }

    result
}

fn find_peak(arr: &[i32]) -> i32 {
    if arr.is_empty() {
        return 0;
    }

    for i in 0..arr.len() {
        let left_greater = i == 0 || arr[i] > arr[i - 1];
        let right_greater = i == arr.len() - 1 || arr[i] > arr[i + 1];

        if left_greater && right_greater {
            return arr[i];
        }
    }

    0
}

fn partition(arr: &mut [i32], low: usize, high: usize) -> usize {
    let pivot = arr[high];
    let mut i = low as i32 - 1;

    for j in low..high {
        if arr[j] < pivot {
            i += 1;
            arr.swap(i as usize, j);
        }
    }

    let pivot_pos = (i + 1) as usize;
    arr.swap(pivot_pos, high);
    pivot_pos
}

fn quick_sort_helper(arr: &mut [i32], low: usize, high: usize) {
    if low < high {
        let pi = partition(arr, low, high);
        if pi > 0 {
            quick_sort_helper(arr, low, pi - 1);
        }
        quick_sort_helper(arr, pi + 1, high);
    }
}

fn quick_sort(arr: &mut [i32]) {
    if arr.is_empty() {
        return;
    }
    quick_sort_helper(arr, 0, arr.len() - 1);
}

fn main() {
    // Test basic array operations
    let arr = vec![3, 7, 2, 9, 1, 5];
    let max_val = find_max(&arr);
    let min_val = find_min(&arr);
    let sum = array_sum(&arr);
    let avg = array_average(&arr);

    // Test sorting algorithms
    let mut bubble_arr = vec![5, 2, 8, 1, 9];
    let mut insertion_arr = vec![5, 2, 8, 1, 9];
    let mut quick_arr = vec![5, 2, 8, 1, 9];

    bubble_sort(&mut bubble_arr);
    insertion_sort(&mut insertion_arr);
    quick_sort(&mut quick_arr);

    // Test binary search
    let sorted_arr = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
    let search_result = binary_search(&sorted_arr, 5);

    // Test merge arrays
    let arr1 = vec![1, 3, 5];
    let arr2 = vec![2, 4, 6];
    let merged = merge_arrays(&arr1, &arr2);

    // Test rotate array
    let mut rotate_arr = vec![1, 2, 3, 4, 5];
    rotate_array(&mut rotate_arr, 2);

    // Test remove duplicates
    let mut dup_arr = vec![1, 1, 2, 2, 3, 3, 3];
    let unique_arr = remove_duplicates(&mut dup_arr);

    // Test 2D array operations
    let matrix = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
    let matrix_total = matrix_sum(&matrix);
    let transposed = transpose_matrix(&matrix);

    // Test find peak
    let peak_arr = vec![1, 3, 2, 4, 5, 2, 1];
    let peak = find_peak(&peak_arr);

    println!(
        "Array operations test: {} {} {} {}",
        max_val, min_val, sum, unique_arr.len()
    );
    println!(
        "Average: {:.2}, Search result: {}, Merged size: {}",
        avg,
        search_result,
        merged.len()
    );
    println!(
        "Matrix sum: {}, Transposed size: {}x{}",
        matrix_total,
        transposed.len(),
        if transposed.is_empty() { 0 } else { transposed[0].len() }
    );
    println!(
        "Quick sort size: {}, Insertion sort size: {}, Peak: {}",
        quick_arr.len(),
        insertion_arr.len(),
        peak
    );
}
