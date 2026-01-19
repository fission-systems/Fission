fn main() {
    let message = "Hello, Fission!";
    println!("{}", message);
    complex_function(42);
}

#[inline(never)]
fn complex_function(val: i32) {
    let result = (val * 2) + 7;
    println!("Result: {}", result);

    let mut vec = Vec::new();
    for i in 0..val {
        vec.push(i);
    }
    println!("Vector size: {}", vec.len());
}
