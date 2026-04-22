use std::f64::consts::PI;

fn gcd(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a
}

fn lcm(a: i32, b: i32) -> i32 {
    (a / gcd(a, b)) * b
}

fn is_prime(n: i32) -> bool {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }

    let mut i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

fn factorial(n: u32) -> u64 {
    match n {
        0 | 1 => 1,
        _ => (2..=n as u64).product(),
    }
}

fn power(base: i32, exp: i32) -> i64 {
    (0..exp).fold(1i64, |acc, _| acc * base as i64)
}

fn mod_power(mut base: i64, mut exp: i64, modulus: i64) -> i64 {
    let mut result = 1i64;
    base %= modulus;

    while exp > 0 {
        if exp % 2 == 1 {
            result = (result * base) % modulus;
        }
        exp >>= 1;
        base = (base * base) % modulus;
    }

    result
}

fn sum_of_digits(mut n: i32) -> i32 {
    let mut sum = 0;
    if n < 0 {
        n = -n;
    }

    while n > 0 {
        sum += n % 10;
        n /= 10;
    }

    sum
}

fn count_digits(mut n: i32) -> i32 {
    if n == 0 {
        return 1;
    }
    if n < 0 {
        n = -n;
    }

    let mut count = 0;
    while n > 0 {
        count += 1;
        n /= 10;
    }
    count
}

fn reverse_integer(mut n: i32) -> i32 {
    let mut reversed = 0;
    let is_negative = n < 0;
    if is_negative {
        n = -n;
    }

    while n > 0 {
        reversed = reversed * 10 + (n % 10);
        n /= 10;
    }

    if is_negative {
        -reversed
    } else {
        reversed
    }
}

fn is_palindrome(n: i32) -> bool {
    if n < 0 {
        false
    } else {
        n == reverse_integer(n)
    }
}

fn circle_area(radius: f64) -> f64 {
    PI * radius * radius
}

fn circle_circumference(radius: f64) -> f64 {
    2.0 * PI * radius
}

fn sphere_volume(radius: f64) -> f64 {
    (4.0 / 3.0) * PI * radius * radius * radius
}

fn calculate_mean(arr: &[i32]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    arr.iter().sum::<i32>() as f64 / arr.len() as f64
}

fn calculate_variance(arr: &[i32]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }

    let mean = calculate_mean(arr);
    let variance: f64 = arr
        .iter()
        .map(|&x| {
            let diff = x as f64 - mean;
            diff * diff
        })
        .sum();

    variance / arr.len() as f64
}

#[derive(Clone, Copy)]
struct Matrix2x2 {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
}

impl Matrix2x2 {
    fn new(a: f64, b: f64, c: f64, d: f64) -> Self {
        Matrix2x2 { a, b, c, d }
    }

    fn multiply(self, m: Matrix2x2) -> Matrix2x2 {
        Matrix2x2 {
            a: self.a * m.a + self.b * m.c,
            b: self.a * m.b + self.b * m.d,
            c: self.c * m.a + self.d * m.c,
            d: self.c * m.b + self.d * m.d,
        }
    }

    fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }
}

fn fibonacci(n: i32, cache: &mut std::collections::HashMap<i32, i32>) -> i32 {
    if n <= 1 {
        return n;
    }
    if let Some(&val) = cache.get(&n) {
        return val;
    }

    let result = fibonacci(n - 1, cache) + fibonacci(n - 2, cache);
    cache.insert(n, result);
    result
}

fn main() {
    // Test number theory
    let gcd_result = gcd(48, 18);
    let lcm_result = lcm(12, 18);
    let is_prime_result = is_prime(17);

    // Test factorial
    let fact_result = factorial(5);

    // Test power
    let pow_result = power(2, 10);
    let mod_pow_result = mod_power(2, 100, 1000000007);

    // Test digit operations
    let digit_sum = sum_of_digits(12345);
    let digit_count = count_digits(98765);
    let reversed = reverse_integer(12345);
    let is_palin = is_palindrome(121);

    // Test geometry
    let circle_area_result = circle_area(5.0);
    let circle_circum_result = circle_circumference(5.0);
    let sphere_vol_result = sphere_volume(3.0);

    // Test statistics
    let data = vec![10, 20, 30, 40, 50];
    let mean = calculate_mean(&data);
    let variance = calculate_variance(&data);

    // Test matrix operations
    let m1 = Matrix2x2::new(1.0, 2.0, 3.0, 4.0);
    let m2 = Matrix2x2::new(5.0, 6.0, 7.0, 8.0);
    let product = m1.multiply(m2);
    let det = m1.determinant();

    // Test Fibonacci with memoization
    let mut cache = std::collections::HashMap::new();
    let fib_result = fibonacci(15, &mut cache);

    println!(
        "Math operations test: {} {} {} {}",
        gcd_result, lcm_result, fact_result, digit_sum
    );
    println!("Powers: {} {}, Fibonacci: {}", pow_result, mod_pow_result, fib_result);
    println!(
        "Geometry: area={:.2}, circumference={:.2}, volume={:.2}",
        circle_area_result, circle_circum_result, sphere_vol_result
    );
    println!(
        "Statistics: mean={:.2}, variance={:.2}",
        mean, variance
    );
    println!(
        "Matrix determinant: {:.2}, Product a: {:.2}",
        det, product.a
    );
    println!("Palindrome: {}, Digits: {}", is_palin, digit_count);
}
